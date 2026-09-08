//! The Spectravideo SVI-328 launcher: runtime-owned models and firmware, parsed cartridge media.

use std::path::PathBuf;

use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom};
use emu198x_shell::{FirmwareOverrides, MediaKind, build_variant, build_variant_or_blank};
use runtime_spectravideo_svi_328::{Model, Svi328Runtime, Svi328SessionQueryProvider};
use serde_json::{Map, Value};

#[derive(Debug, PartialEq, Eq)]
pub struct Svi328 {
    pub firmware: FirmwareOverrides,
    pub cart: Option<PathBuf>,
    pub model: Model,
}

impl Default for Svi328 {
    fn default() -> Self {
        Self {
            firmware: FirmwareOverrides::none(),
            cart: None,
            model: Model::Svi328Ntsc,
        }
    }
}

impl MachineApp for Svi328 {
    type Runtime = Svi328Runtime;
    type Query = Svi328SessionQueryProvider;
    const BIN_NAME: &'static str = "emu198x-spectravideo-svi-328";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str = "    --rom PATH|ID=PATH BASIC/OS ROM (32 KB); default
                    ~/.emu198x/roms/spectravideo-svi-328/svi-328.rom
                    (or set EMU198X_SVI_328_BIOS)
    --rom-dir DIR   firmware directory (or set EMU198X_SVI_328_ROM_DIR)
                    firmware ID: spectravideo-svi-328-rom
    --model ID      spectravideo-svi-328-ntsc | spectravideo-svi-328-pal
    --region MODE   ntsc | pal [default: ntsc]; last selector wins
    --cart PATH     cartridge ROM (up to 16 KB at $8000-$BFFF)
    --bios PATH     legacy alias for the system firmware path";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the SVI-328 keyboard (cursor keys are real SVI keys)
    Shift / Ctrl    the SVI SHIFT / CTRL keys (Alt = GRAPH/CODE)
    Gamepad         joystick (player 1)";

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
            "--bios" => {
                self.firmware.by_id.insert(
                    runtime_spectravideo_svi_328::BIOS_FIRMWARE_ID.to_owned(),
                    args.path(flag)?,
                );
            }
            "--cart" => self.cart = Some(args.path(flag)?),
            "--model" => {
                let id = args.value(flag)?;
                self.model = Model::from_variant_id(&id).ok_or_else(|| {
                    LaunchError::Usage(format!(
                        "unknown spectravideo-svi-328 model `{id}`; expected {}",
                        Model::VARIANT_IDS.join(", ")
                    ))
                })?;
            }
            "--region" => {
                self.model = match args.value(flag)?.as_str() {
                    "ntsc" => Model::Svi328Ntsc,
                    "pal" => Model::Svi328Pal,
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

    fn query_provider(&self) -> Svi328SessionQueryProvider {
        Svi328SessionQueryProvider
    }

    fn build_runtime(&self) -> Result<Svi328Runtime, LaunchError> {
        build_variant::<Svi328Runtime>(self.model, &self.firmware)
            .map_err(|err| LaunchError::Run(err.to_string()))
    }

    /// Missing conventional firmware permits blank MCP startup; explicit errors fail.
    fn build_mcp_runtime(&self) -> Result<Svi328Runtime, LaunchError> {
        build_variant_or_blank(self.model, &self.firmware, Svi328Runtime::blank)
            .map_err(|err| LaunchError::Run(err.to_string()))
    }

    fn startup_media(&self) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        let Some(path) = &self.cart else {
            return Ok(Vec::new());
        };
        Ok(vec![(
            "cartridge-1".to_owned(),
            MediaKind::Cartridge,
            read_rom(path, "--cart")?,
        )])
    }

    fn mcp_startup_media(
        &self,
        _slots: &[emu198x_shell::MediaSlot],
        _raw_args: &[String],
    ) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        self.startup_media()
    }

    fn report(&self, runtime: &Svi328Runtime, report: &mut Map<String, Value>) {
        report.insert("bios_loaded".to_owned(), runtime.machine().is_some().into());
        report.insert("cart_loaded".to_owned(), runtime.cartridge_loaded().into());
        report.insert(
            "frames_run".to_owned(),
            runtime.machine().map_or(0, |m| m.frame_count()).into(),
        );
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
        let Parsed::Run { app, .. } = parse::<Svi328>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.firmware.by_id.is_empty());
        assert!(app.cart.is_none());
        assert_eq!(app.model, Model::Svi328Ntsc);
    }

    #[test]
    fn parse_cli_accepts_bios_cart_region_scale_video() {
        let parsed = parse::<Svi328>(&args(&[
            "--bios", "svi.rom", "--cart", "game.rom", "--region", "pal", "--scale", "4",
            "--video", "crt",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(
            app.firmware
                .by_id
                .get(runtime_spectravideo_svi_328::BIOS_FIRMWARE_ID),
            Some(&PathBuf::from("svi.rom"))
        );
        assert_eq!(app.cart, Some(PathBuf::from("game.rom")));
        assert_eq!(app.model, Model::Svi328Pal);
        assert_eq!(common.scale, Some(4));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn a_bad_region_is_a_usage_error() {
        let err = parse::<Svi328>(&args(&["--region", "secam"])).expect_err("rejects");
        assert_eq!(
            err,
            LaunchError::Usage("--region expects ntsc|pal, got secam".to_owned())
        );
    }

    #[test]
    fn region_frame_ticks_match() {
        assert_eq!(Model::Svi328Ntsc.frame_ticks(), 228 * 262);
        assert_eq!(Model::Svi328Pal.frame_ticks(), 228 * 313);
    }
}
