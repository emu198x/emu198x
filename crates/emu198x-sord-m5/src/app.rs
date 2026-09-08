//! The Sord M5 launcher: runtime-owned models and firmware, parsed cartridge media.

use std::path::PathBuf;

use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom};
use emu198x_shell::{FirmwareOverrides, MediaKind, build_variant, build_variant_or_blank};
use runtime_sord_m5::{M5Runtime, M5SessionQueryProvider, Model};
use serde_json::{Map, Value};

#[derive(Debug, PartialEq, Eq)]
pub struct SordM5 {
    pub firmware: FirmwareOverrides,
    pub cart: Option<PathBuf>,
    pub model: Model,
}

impl Default for SordM5 {
    fn default() -> Self {
        Self {
            firmware: FirmwareOverrides::none(),
            cart: None,
            model: Model::M5Ntsc,
        }
    }
}

impl MachineApp for SordM5 {
    type Runtime = M5Runtime;
    type Query = M5SessionQueryProvider;
    const BIN_NAME: &'static str = "emu198x-sord-m5";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str = "    --rom PATH|ID=PATH monitor ROM (8 KB); default
                    ~/.emu198x/roms/sord-m5/sord-m5.rom
                    (or set EMU198X_SORD_M5_ROM)
    --rom-dir DIR   firmware directory (or set EMU198X_SORD_M5_ROM_DIR)
                    firmware ID: sord-m5-rom
    --model ID      sord-m5-ntsc | sord-m5-pal
    --region MODE   ntsc | pal [default: ntsc]; last selector wins
    --cart PATH     cartridge ROM (optional)";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the M5 keyboard
    Shift / Ctrl    the M5 SHIFT / CONTROL keys (Tab = FUNC)
    Gamepad         joystick (player 1, directions)";

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
            "--cart" => self.cart = Some(args.path(flag)?),
            "--model" => {
                let id = args.value(flag)?;
                self.model = Model::from_variant_id(&id).ok_or_else(|| {
                    LaunchError::Usage(format!(
                        "unknown sord-m5 model `{id}`; expected {}",
                        Model::VARIANT_IDS.join(", ")
                    ))
                })?;
            }
            "--region" => {
                self.model = match args.value(flag)?.as_str() {
                    "ntsc" => Model::M5Ntsc,
                    "pal" => Model::M5Pal,
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

    fn query_provider(&self) -> M5SessionQueryProvider {
        M5SessionQueryProvider
    }

    fn build_runtime(&self) -> Result<M5Runtime, LaunchError> {
        build_variant::<M5Runtime>(self.model, &self.firmware)
            .map_err(|err| LaunchError::Run(err.to_string()))
    }

    /// Missing conventional firmware permits blank MCP startup; explicit errors fail.
    fn build_mcp_runtime(&self) -> Result<M5Runtime, LaunchError> {
        build_variant_or_blank(self.model, &self.firmware, M5Runtime::blank)
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

    fn report(&self, runtime: &M5Runtime, report: &mut Map<String, Value>) {
        report.insert("rom_loaded".to_owned(), runtime.machine().is_some().into());
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
    fn flags_set_rom_cart_and_region() {
        let parsed = parse::<SordM5>(&args(&[
            "--rom", "m5.rom", "--cart", "game.rom", "--region", "pal", "--scale", "4",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(
            app.firmware.by_id.get(runtime_sord_m5::ROM_FIRMWARE_ID),
            Some(&PathBuf::from("m5.rom"))
        );
        assert_eq!(app.cart, Some(PathBuf::from("game.rom")));
        assert_eq!(app.model, Model::M5Pal);
        assert_eq!(common.scale, Some(4));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn a_bad_region_is_a_usage_error() {
        let err = parse::<SordM5>(&args(&["--region", "secam"])).expect_err("rejects");
        assert_eq!(
            err,
            LaunchError::Usage("--region expects ntsc|pal, got secam".to_owned())
        );
    }

    #[test]
    fn region_frame_ticks_match() {
        assert_eq!(Model::M5Ntsc.frame_ticks(), 228 * 262);
        assert_eq!(Model::M5Pal.frame_ticks(), 228 * 313);
    }
}
