//! The Atari 800XL as a [`MachineApp`]: its flags, runtime, and report fields.

use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom};
use emu198x_shell::{FirmwareOverrides, MediaKind, build_variant, read_media_asset};
use runtime_atari_800xl::{
    Atari800xlRuntime, Atari800xlSessionQueryProvider, BASIC_FIRMWARE_ID, Model, OS_FIRMWARE_ID,
};
use serde_json::{Map, Value};
use std::path::PathBuf;

/// The machine configuration the flags build up.
#[derive(Debug, PartialEq, Eq)]
pub struct Atari800xl {
    pub firmware: FirmwareOverrides,
    pub cart: Option<PathBuf>,
    /// `--disk PATH`: an ATR image for D1:, loaded before a script runs.
    pub disk: Option<PathBuf>,
    /// `--no-basic` clears this: OPTION is held at boot to disable BASIC.
    pub basic_enabled: bool,
    pub model: Model,
}

impl Default for Atari800xl {
    fn default() -> Self {
        Self {
            firmware: FirmwareOverrides::none(),
            cart: None,
            disk: None,
            basic_enabled: true,
            model: Model::A800xlNtsc,
        }
    }
}

impl Atari800xl {
    fn configured_runtime(&self) -> Result<Atari800xlRuntime, LaunchError> {
        let mut runtime = build_variant::<Atari800xlRuntime>(self.model, &self.firmware)
            .map_err(|err| LaunchError::Run(err.to_string()))?;
        runtime
            .set_basic_enabled(self.basic_enabled)
            .map_err(|err| LaunchError::Run(err.to_string()))?;
        if let Some(path) = &self.cart {
            runtime
                .insert_cartridge(Some(read_rom(path, "--cart")?))
                .map_err(|err| LaunchError::Run(err.to_string()))?;
        }
        Ok(runtime)
    }
}

impl MachineApp for Atari800xl {
    type Runtime = Atari800xlRuntime;
    type Query = Atari800xlSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-atari-800xl";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str = "    --os PATH       16 KB OS ROM; default
                    ~/.emu198x/roms/atari-800xl/atarixl.rom (or EMU198X_A800XL_OS)
    --basic PATH    8 KB Atari BASIC ROM; default
                    ~/.emu198x/roms/atari-800xl/ataribas.rom (or EMU198X_A800XL_BASIC)
    --rom ID=PATH   pin an OS or BASIC catalogue image
    --rom-dir DIR   firmware directory (or EMU198X_A800XL_ROM_DIR)
    --model ID      atari-800xl-ntsc | atari-800xl-pal
    --cart PATH     cartridge image (flat, XEGS, MegaCart or OSS; .car headers honoured)
    --disk PATH     ATR disk image for D1: (a .zip holding one .atr works too)
    --no-basic      hold OPTION at boot to disable the built-in BASIC
    --region MODE   ntsc | pal [default: ntsc]";
    const CONTROLS: &'static str = "    Esc             quit
    F12             emulator hard reset
    A-Z 0-9 etc.    the Atari keyboard
    Enter / Space / Delete / Tab   the matching Atari keys
    Arrow keys      joystick (player 1)
    F2 / F3 / F4    Start / Select / Option console keys
    Gamepad         joystick + fire (player 1)";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--os" => self.firmware.pin(OS_FIRMWARE_ID, args.path(flag)?),
            "--basic" => self.firmware.pin(BASIC_FIRMWARE_ID, args.path(flag)?),
            "--rom-dir" => self.firmware.dir = Some(args.path(flag)?),
            "--rom" => self
                .firmware
                .add_spec(
                    &args.value(flag)?,
                    self.model.variant_id(),
                    &self.model.firmware_sources(),
                )
                .map_err(|err| LaunchError::Usage(err.to_string()))?,
            "--model" => {
                let id = args.value(flag)?;
                self.model = Model::from_variant_id(&id).ok_or_else(|| {
                    LaunchError::Usage(format!(
                        "unknown Atari 800XL model `{id}`; expected {}",
                        Model::VARIANT_IDS.join(", ")
                    ))
                })?;
            }
            "--cart" => self.cart = Some(args.path(flag)?),
            "--disk" => self.disk = Some(args.path(flag)?),
            "--no-basic" => self.basic_enabled = false,
            "--region" => {
                self.model = match args.value(flag)?.as_str() {
                    "ntsc" => Model::A800xlNtsc,
                    "pal" => Model::A800xlPal,
                    other => {
                        return Err(LaunchError::Usage(format!(
                            "--region expects ntsc|pal, got {other}"
                        )));
                    }
                };
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        self.model.frame_ticks()
    }

    fn query_provider(&self) -> Atari800xlSessionQueryProvider {
        Atari800xlSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<Atari800xlRuntime, LaunchError> {
        let runtime = self.configured_runtime()?;
        if runtime.machine().is_none() {
            return Err(LaunchError::Run("either --os or --cart must be provided (cart-only boot uses the cart's reset vector)".to_owned()));
        }
        Ok(runtime)
    }

    fn build_mcp_runtime(&self) -> Result<Atari800xlRuntime, LaunchError> {
        self.configured_runtime()
    }

    fn mcp_startup_media(
        &self,
        _slots: &[emu198x_shell::MediaSlot],
        _raw_args: &[String],
    ) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        self.startup_media()
    }

    fn startup_media(&self) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        let Some(path) = &self.disk else {
            return Ok(Vec::new());
        };
        let loaded = read_media_asset(path, MediaKind::Disk).map_err(|err| {
            LaunchError::Run(format!(
                "failed to load disk asset {}: {err}",
                path.display()
            ))
        })?;
        Ok(vec![("disk-1".to_owned(), MediaKind::Disk, loaded.bytes)])
    }

    fn report(&self, runtime: &Atari800xlRuntime, report: &mut Map<String, Value>) {
        let machine_loaded = runtime.machine().is_some();
        let frames_run = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("machine_loaded".to_owned(), machine_loaded.into());
        report.insert("frames_run".to_owned(), frames_run.into());
        report.insert("basic_enabled".to_owned(), runtime.basic_enabled().into());
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
    fn defaults_to_ntsc_with_basic() {
        let parsed = parse::<Atari800xl>(&[]).expect("parses");
        let Parsed::Run { app, mode, .. } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.model, Model::A800xlNtsc);
        assert!(app.basic_enabled);
        assert!(app.disk.is_none());
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn flags_set_roms_disk_basic_and_region() {
        let parsed = parse::<Atari800xl>(&args(&[
            "--cart",
            "game.bin",
            "--disk",
            "dos.atr",
            "--no-basic",
            "--region",
            "pal",
        ]))
        .expect("parses");
        let Parsed::Run { app, mode, .. } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.cart, Some(PathBuf::from("game.bin")));
        assert_eq!(app.disk, Some(PathBuf::from("dos.atr")));
        assert!(!app.basic_enabled);
        assert_eq!(app.model, Model::A800xlPal);
        // A bare `--cart` is shared with the UI, so it opens the window.
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn a_bad_region_is_a_usage_error() {
        let err = parse::<Atari800xl>(&args(&["--region", "secam"])).expect_err("rejects");
        assert_eq!(
            err,
            LaunchError::Usage("--region expects ntsc|pal, got secam".to_owned())
        );
    }

    #[test]
    fn region_frame_ticks_match() {
        assert_eq!(Model::A800xlNtsc.frame_ticks(), 262 * 228);
        assert_eq!(Model::A800xlPal.frame_ticks(), 312 * 228);
    }
}
