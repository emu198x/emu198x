//! The ColecoVision as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::PathBuf;

use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom};
use emu198x_shell::{FirmwareOverrides, MediaKind, build_variant, build_variant_or_blank};
use runtime_coleco_colecovision::{CvRuntime, CvSessionQueryProvider, Model};
use serde_json::{Map, Value};

/// Parsed launch options; the runtime owns model and firmware definitions.
#[derive(Debug, PartialEq, Eq)]
pub struct ColecoVision {
    pub firmware: FirmwareOverrides,
    pub cart: Option<PathBuf>,
    pub model: Model,
}

impl Default for ColecoVision {
    fn default() -> Self {
        Self {
            firmware: FirmwareOverrides::none(),
            cart: None,
            model: Model::CvNtsc,
        }
    }
}

impl MachineApp for ColecoVision {
    type Runtime = CvRuntime;
    type Query = CvSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-colecovision";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --bios PATH     ColecoVision BIOS ROM (8 KB); default
                    ~/.emu198x/roms/coleco-colecovision/colecovision.rom
                    (or set EMU198X_COLECO_BIOS)
    --rom PATH|ID=PATH BIOS pin (ID: colecovision-bios)
    --rom-dir DIR   BIOS directory (or set EMU198X_COLECO_ROM_DIR)
    --cart PATH     cartridge ROM image (optional — BIOS shows the splash;
                    a bare PATH is accepted too)
    --model ID      coleco-colecovision-ntsc | coleco-colecovision-pal
    --region MODE   ntsc | pal [default: ntsc]; last selector wins";
    const CONTROLS: &'static str = "    Esc             quit
    F12             emulator hard reset
    Arrow keys      joystick (player 1)
    Z / X           left and right fire buttons
    0-9             numeric keypad
    Numpad * / /    keypad * and # keys";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--bios" => {
                self.firmware.by_id.insert(
                    runtime_coleco_colecovision::BIOS_FIRMWARE_ID.to_owned(),
                    args.path(flag)?,
                );
            }
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
                        "unknown colecovision model `{id}`; expected {}",
                        Model::VARIANT_IDS.join(", ")
                    ))
                })?;
            }
            "--region" => {
                self.model = match args.value(flag)?.as_str() {
                    "ntsc" => Model::CvNtsc,
                    "pal" => Model::CvPal,
                    other => {
                        return Err(LaunchError::Usage(format!(
                            "--region expects ntsc|pal, got {other}"
                        )));
                    }
                };
            }
            _ if flag.starts_with('-') => return Ok(false),
            _ if self.cart.is_none() => self.cart = Some(PathBuf::from(flag)),
            _ => {
                return Err(LaunchError::Usage(
                    "only one positional cart path is supported".to_owned(),
                ));
            }
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        self.model.frame_ticks()
    }

    fn query_provider(&self) -> CvSessionQueryProvider {
        CvSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<CvRuntime, LaunchError> {
        build_variant::<CvRuntime>(self.model, &self.firmware)
            .map_err(|err| LaunchError::Run(err.to_string()))
    }

    fn build_mcp_runtime(&self) -> Result<CvRuntime, LaunchError> {
        build_variant_or_blank(self.model, &self.firmware, CvRuntime::blank)
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

    fn report(&self, runtime: &CvRuntime, report: &mut Map<String, Value>) {
        let bios_loaded = runtime.machine().is_some();
        let frame_count = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("bios_loaded".to_owned(), bios_loaded.into());
        report.insert("cart_loaded".to_owned(), runtime.cartridge_loaded().into());
        report.insert("frames_run".to_owned(), frame_count.into());
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
        let Parsed::Run { app, common, .. } = parse::<ColecoVision>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.firmware.by_id.is_empty());
        assert!(app.cart.is_none());
        assert_eq!(app.model, Model::CvNtsc);
        assert_eq!(common.frames, 0);
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let parsed = parse::<ColecoVision>(&args(&[
            "--bios",
            "/tmp/bios",
            "--cart",
            "/tmp/cart",
            "--region",
            "pal",
            "--frames",
            "120",
            "--screenshot",
            "/tmp/shot.png",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(
            app.firmware
                .by_id
                .get(runtime_coleco_colecovision::BIOS_FIRMWARE_ID)
                .map(PathBuf::as_path),
            Some(Path::new("/tmp/bios"))
        );
        assert_eq!(app.cart.as_deref(), Some(Path::new("/tmp/cart")));
        assert_eq!(app.model, Model::CvPal);
        assert_eq!(common.frames, 120);
        assert_eq!(mode, Mode::Script);
    }

    #[test]
    fn parse_cli_accepts_bios_cart_region_scale_video() {
        let parsed = parse::<ColecoVision>(&args(&[
            "--bios",
            "coleco.rom",
            "--cart",
            "dk.col",
            "--region",
            "pal",
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
            app.firmware
                .by_id
                .get(runtime_coleco_colecovision::BIOS_FIRMWARE_ID),
            Some(&PathBuf::from("coleco.rom"))
        );
        assert_eq!(app.cart, Some(PathBuf::from("dk.col")));
        assert_eq!(app.model, Model::CvPal);
        assert_eq!(common.scale, Some(4));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn parse_cli_accepts_positional_cart() {
        let Parsed::Run { app, .. } = parse::<ColecoVision>(&args(&["dk.col"])).expect("parses")
        else {
            panic!("expected a run");
        };
        assert_eq!(app.cart, Some(PathBuf::from("dk.col")));
    }

    #[test]
    fn last_model_or_region_selector_wins() {
        for (flags, expected) in [
            (
                vec!["--region", "pal", "--model", Model::CvNtsc.variant_id()],
                Model::CvNtsc,
            ),
            (
                vec!["--model", Model::CvNtsc.variant_id(), "--region", "pal"],
                Model::CvPal,
            ),
        ] {
            let Parsed::Run { app, .. } = parse::<ColecoVision>(&args(&flags)).expect("parses")
            else {
                panic!("run");
            };
            assert_eq!(app.model, expected);
        }
    }

    #[test]
    fn region_frame_ticks_match() {
        assert_eq!(Model::CvNtsc.frame_ticks(), 228 * 262);
        assert_eq!(Model::CvPal.frame_ticks(), 228 * 313);
    }
}
