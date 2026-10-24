//! The Super NES as a [`MachineApp`]: its flags, runtime and report fields.

use std::path::{Path, PathBuf};

use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom};
use emu198x_shell::{FirmwareOverrides, build_variant};
use runtime_nintendo_snes::cartridge::destination;
use runtime_nintendo_snes::{Model, SnesRuntime, SnesSessionQueryProvider};
use serde_json::{Map, Value};

/// The model when neither the flags nor the cartridge choose one.
const DEFAULT_MODEL: Model = Model::SnesNtsc;

/// The machine configuration the flags build up.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Snes {
    /// `--cart PATH`, or the one positional argument.
    pub cart: Option<PathBuf>,
    /// `--model ID`, when given.
    pub model: Option<Model>,
}

impl Snes {
    fn cart_path(&self) -> Result<&Path, LaunchError> {
        self.cart
            .as_deref()
            .ok_or_else(|| LaunchError::Run("provide a cartridge with --cart PATH".to_owned()))
    }

    /// The model to run: the one asked for, else the one the cartridge's
    /// header names, else the NTSC Super NES. The header is read from the
    /// file here, so the launcher's frame budget and the runtime agree.
    pub fn model(&self) -> Model {
        self.model.unwrap_or_else(|| {
            self.cart
                .as_deref()
                .and_then(|path| std::fs::read(path).ok())
                .and_then(|image| destination(&image))
                .map_or(DEFAULT_MODEL, |(code, pal)| {
                    Model::for_destination(code, pal)
                })
        })
    }
}

impl MachineApp for Snes {
    type Runtime = SnesRuntime;
    type Query = SnesSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-snes";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --cart PATH     cartridge image, .sfc or .smc with or without a
                    copier header (required; a bare PATH is accepted too)
    --model ID      snes-ntsc | snes-pal | super-famicom, or a full
                    profile id (alias: --variant) [default: the
                    cartridge header's destination, else snes-ntsc]";
    const CONTROLS: &'static str = "    Close window    quit (or use the platform Quit shortcut)
    F12             emulator hard reset
    Arrow keys      d-pad (player 1)
    Z / X           B and A
    A / S           Y and X
    Q / W           L and R
    Enter           Start
    Right Shift     Select";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--cart" => self.cart = Some(args.path(flag)?),
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
        self.model().frame_ticks()
    }

    fn query_provider(&self) -> SnesSessionQueryProvider {
        SnesSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<SnesRuntime, LaunchError> {
        let cart = read_rom(self.cart_path()?, "--cart")?;
        let mut runtime = build_variant::<SnesRuntime>(self.model(), &FirmwareOverrides::none())
            .map_err(|err| LaunchError::Run(err.to_string()))?;
        runtime
            .insert_cartridge(cart)
            .map_err(|err| LaunchError::Run(err.to_string()))?;
        Ok(runtime)
    }

    /// Without a cartridge MCP starts empty and waits for load_media.
    fn build_mcp_runtime(&self) -> Result<SnesRuntime, LaunchError> {
        if self.cart.is_some() {
            return self.build_runtime();
        }
        build_variant::<SnesRuntime>(self.model(), &FirmwareOverrides::none())
            .map_err(|err| LaunchError::Run(err.to_string()))
    }

    fn mcp_startup_media(
        &self,
        _slots: &[emu198x_shell::MediaSlot],
        _raw_args: &[String],
    ) -> Result<Vec<(String, emu198x_shell::MediaKind, Vec<u8>)>, LaunchError> {
        // build_mcp_runtime already loaded the parsed cartridge.
        self.startup_media()
    }

    fn report(&self, runtime: &SnesRuntime, report: &mut Map<String, Value>) {
        let cart_loaded = runtime.machine().is_some();
        let frame_count = runtime.machine().map_or(0, |m| m.ppu.frames());
        report.insert("cart_loaded".to_owned(), cart_loaded.into());
        report.insert("frames_run".to_owned(), frame_count.into());
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
        let Parsed::Run { app, common, .. } = parse::<Snes>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.cart.is_none());
        assert_eq!(app.model, None);
        assert_eq!(app.model(), Model::SnesNtsc);
        assert_eq!(common.frames, 0);
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let parsed = parse::<Snes>(&args(&[
            "--cart",
            "/tmp/cart.sfc",
            "--model",
            "snes-pal",
            "--frames",
            "60",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.cart.as_deref(), Some(Path::new("/tmp/cart.sfc")));
        assert_eq!(app.model, Some(Model::SnesPal));
        assert_eq!(common.frames, 60);
        assert_eq!(mode, Mode::Script);
    }

    #[test]
    fn parse_cli_accepts_positional_cart_and_variant_alias() {
        let Parsed::Run { app, mode, .. } =
            parse::<Snes>(&args(&["game.smc", "--variant", "super-famicom"])).expect("parses")
        else {
            panic!("expected a run");
        };
        assert_eq!(app.cart, Some(PathBuf::from("game.smc")));
        assert_eq!(app.model, Some(Model::SuperFamicom));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn variant_frame_ticks_match() {
        let app = |model| Snes {
            cart: None,
            model: Some(model),
        };
        assert_eq!(app(Model::SnesNtsc).frame_ticks(), 262 * 1364);
        assert_eq!(app(Model::SnesPal).frame_ticks(), 312 * 1364);
    }
}
