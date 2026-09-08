//! The Sega Game Gear as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::PathBuf;

use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom};
use emu198x_shell::{FirmwareOverrides, build_variant};
use runtime_sega_game_gear::{Model, SmsRuntime, SmsSessionQueryProvider};
use serde_json::{Map, Value};

/// The machine configuration the flags build up.
#[derive(Debug, PartialEq, Eq)]
pub struct GameGear {
    /// `--cart PATH`, or the one positional argument.
    pub cart: Option<PathBuf>,
    pub model: Model,
}

impl Default for GameGear {
    fn default() -> Self {
        Self {
            cart: None,
            model: Model::GameGear,
        }
    }
}

impl MachineApp for GameGear {
    type Runtime = SmsRuntime;
    type Query = SmsSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-sega-game-gear";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --cart PATH     cartridge ROM (required; a bare PATH is accepted too)
    --model ID      canonical model id or full profile id (alias: --variant)
    --variant KIND  game-gear [default: game-gear]";
    const CONTROLS: &'static str = "    Esc             quit
    F12             emulator hard reset
    Arrow keys      d-pad (player 1)
    Z / X           buttons 1 and 2
    Enter           Start";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--cart" => self.cart = Some(args.path(flag)?),
            "--variant" | "--model" => {
                let id = args.value(flag)?;
                self.model = Model::from_variant_id(&id).ok_or_else(|| {
                    LaunchError::Usage(format!(
                        "{flag} expects {}, got {id}",
                        Model::VARIANT_IDS.join("|")
                    ))
                })?;
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

    fn query_provider(&self) -> SmsSessionQueryProvider {
        SmsSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<SmsRuntime, LaunchError> {
        let Some(cart_path) = &self.cart else {
            return Err(LaunchError::Run(
                "provide a cartridge with --cart PATH".to_owned(),
            ));
        };
        let cart = read_rom(cart_path, "--cart")?;
        let mut runtime = build_variant::<SmsRuntime>(self.model, &FirmwareOverrides::none())
            .map_err(|err| LaunchError::Run(err.to_string()))?;
        runtime.insert_cartridge(cart);
        Ok(runtime)
    }

    /// Use the same parsed cartridge and battery-save startup as normal launch.
    /// Without a cartridge MCP starts empty and waits for load_media.
    fn build_mcp_runtime(&self) -> Result<SmsRuntime, LaunchError> {
        if self.cart.is_some() {
            return self.build_runtime();
        }
        build_variant::<SmsRuntime>(self.model, &FirmwareOverrides::none())
            .map_err(|err| LaunchError::Run(err.to_string()))
    }

    fn mcp_startup_media(
        &self,
        _slots: &[emu198x_shell::MediaSlot],
        _raw_args: &[String],
    ) -> Result<Vec<(String, emu198x_shell::MediaKind, Vec<u8>)>, LaunchError> {
        // build_mcp_runtime already loaded the parsed cartridge (and its SRAM).
        self.startup_media()
    }

    fn report(&self, runtime: &SmsRuntime, report: &mut Map<String, Value>) {
        let cart_loaded = runtime.machine().is_some();
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
        let Parsed::Run { app, common, .. } = parse::<GameGear>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.cart.is_none());
        assert_eq!(app.model, Model::GameGear);
        assert_eq!(common.frames, 0);
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let parsed = parse::<GameGear>(&args(&[
            "--cart",
            "/tmp/cart",
            "--variant",
            "game-gear",
            "--frames",
            "60",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.cart.as_deref(), Some(Path::new("/tmp/cart")));
        assert_eq!(app.model, Model::GameGear);
        assert_eq!(common.frames, 60);
        assert_eq!(mode, Mode::Script);
    }

    #[test]
    fn parse_cli_accepts_cart_variant_scale_video() {
        let parsed = parse::<GameGear>(&args(&[
            "--cart",
            "game.gg",
            "--variant",
            "game-gear",
            "--scale",
            "4",
            "--video",
            "crt",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.cart, Some(PathBuf::from("game.gg")));
        assert_eq!(app.model, Model::GameGear);
        assert_eq!(common.scale, Some(4));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn parse_cli_accepts_positional_cart() {
        let Parsed::Run { app, .. } = parse::<GameGear>(&args(&["sonic.gg"])).expect("parses")
        else {
            panic!("expected a run");
        };
        assert_eq!(app.cart, Some(PathBuf::from("sonic.gg")));
        assert_eq!(app.model, Model::GameGear);
    }

    #[test]
    fn an_unknown_variant_is_a_usage_error() {
        let err = parse::<GameGear>(&args(&["--variant", "sms"])).expect_err("rejects");
        assert_eq!(
            err,
            LaunchError::Usage("--variant expects game-gear, got sms".to_owned())
        );
    }

    #[test]
    fn variant_frame_ticks_match() {
        assert_eq!(Model::GameGear.frame_ticks(), 228 * 262);
    }
}
