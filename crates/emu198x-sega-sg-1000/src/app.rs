//! The Sega SG-1000 as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::PathBuf;

use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom};
use emu198x_shell::{FirmwareOverrides, MediaKind, build_variant};
use runtime_sega_sg_1000::{Model, Sg1000Runtime, Sg1000SessionQueryProvider};
use serde_json::{Map, Value};

/// Parsed launch options; the runtime owns model and firmware definitions.
#[derive(Debug, PartialEq, Eq)]
pub struct Sg1000 {
    pub cart: Option<PathBuf>,
    pub model: Model,
}

impl Default for Sg1000 {
    fn default() -> Self {
        Self {
            cart: None,
            model: Model::Sg1000Ntsc,
        }
    }
}

impl MachineApp for Sg1000 {
    type Runtime = Sg1000Runtime;
    type Query = Sg1000SessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-sega-sg-1000";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --cart PATH     cartridge ROM (required; a bare PATH is accepted too)
    --model ID      sega-sg-1000-ntsc | sega-sg-1000-pal
    --region MODE   ntsc | pal [default: ntsc]; last selector wins";
    const CONTROLS: &'static str = "    Esc             quit
    F12             emulator hard reset
    Arrow keys      d-pad (player 1)
    Z / X           buttons 1 and 2
    Enter           Pause";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--cart" => self.cart = Some(args.path(flag)?),
            "--model" => {
                let id = args.value(flag)?;
                self.model = Model::from_variant_id(&id).ok_or_else(|| {
                    LaunchError::Usage(format!(
                        "unknown sega-sg-1000 model `{id}`; expected {}",
                        Model::VARIANT_IDS.join(", ")
                    ))
                })?;
            }
            "--region" => {
                self.model = match args.value(flag)?.as_str() {
                    "ntsc" => Model::Sg1000Ntsc,
                    "pal" => Model::Sg1000Pal,
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

    fn query_provider(&self) -> Sg1000SessionQueryProvider {
        Sg1000SessionQueryProvider
    }

    fn build_runtime(&self) -> Result<Sg1000Runtime, LaunchError> {
        if self.cart.is_none() {
            return Err(LaunchError::Run(
                "provide a cartridge with --cart PATH".to_owned(),
            ));
        }
        build_variant::<Sg1000Runtime>(self.model, &FirmwareOverrides::none())
            .map_err(|err| LaunchError::Run(err.to_string()))
    }

    fn build_mcp_runtime(&self) -> Result<Sg1000Runtime, LaunchError> {
        build_variant::<Sg1000Runtime>(self.model, &FirmwareOverrides::none())
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

    fn report(&self, runtime: &Sg1000Runtime, report: &mut Map<String, Value>) {
        let cart_loaded = runtime.cartridge_loaded();
        let frame_count = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("cart_loaded".to_owned(), cart_loaded.into());
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
        let Parsed::Run { app, common, .. } = parse::<Sg1000>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.cart.is_none());
        assert_eq!(app.model, Model::Sg1000Ntsc);
        assert_eq!(common.frames, 0);
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let parsed = parse::<Sg1000>(&args(&[
            "--cart",
            "/tmp/cart",
            "--region",
            "pal",
            "--frames",
            "60",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.cart.as_deref(), Some(Path::new("/tmp/cart")));
        assert_eq!(app.model, Model::Sg1000Pal);
        assert_eq!(common.frames, 60);
        assert_eq!(mode, Mode::Script);
    }

    #[test]
    fn parse_cli_accepts_cart_region_scale_video() {
        let parsed = parse::<Sg1000>(&args(&[
            "--cart", "game.sg", "--region", "pal", "--scale", "4", "--video", "crt",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.cart, Some(PathBuf::from("game.sg")));
        assert_eq!(app.model, Model::Sg1000Pal);
        assert_eq!(common.scale, Some(4));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn parse_cli_accepts_positional_cart() {
        let Parsed::Run { app, .. } = parse::<Sg1000>(&args(&["game.sg"])).expect("parses") else {
            panic!("expected a run");
        };
        assert_eq!(app.cart, Some(PathBuf::from("game.sg")));
        assert_eq!(app.model, Model::Sg1000Ntsc);
    }

    #[test]
    fn last_model_or_region_selector_wins() {
        for (flags, expected) in [
            (
                vec!["--region", "pal", "--model", Model::Sg1000Ntsc.variant_id()],
                Model::Sg1000Ntsc,
            ),
            (
                vec!["--model", Model::Sg1000Ntsc.variant_id(), "--region", "pal"],
                Model::Sg1000Pal,
            ),
        ] {
            let Parsed::Run { app, .. } = parse::<Sg1000>(&args(&flags)).expect("parses") else {
                panic!("run");
            };
            assert_eq!(app.model, expected);
        }
    }

    #[test]
    fn region_frame_ticks_match() {
        assert_eq!(Model::Sg1000Ntsc.frame_ticks(), 228 * 262);
        assert_eq!(Model::Sg1000Pal.frame_ticks(), 228 * 313);
    }
}
