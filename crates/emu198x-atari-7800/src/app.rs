//! The Atari 7800 as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::PathBuf;

use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom};
use emu198x_shell::{FirmwareOverrides, build_variant};
use runtime_atari_7800::{Atari7800Runtime, Atari7800SessionQueryProvider, Model};
use serde_json::{Map, Value};

/// The machine configuration the flags build up.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Atari7800 {
    /// `--cart PATH`, or the one positional argument.
    pub cart: Option<PathBuf>,
    pub model: Model,
}

impl MachineApp for Atari7800 {
    type Runtime = Atari7800Runtime;
    type Query = Atari7800SessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-atari-7800";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --cart PATH     Atari 7800 cartridge ROM (.a78 / .bin) (required; a bare
                    PATH is accepted too)
    --model ID      atari-7800-ntsc | atari-7800-pal
    --region MODE   ntsc | pal [default: ntsc]";
    const CONTROLS: &'static str = "    Esc             quit
    F12             emulator hard reset
    Arrow keys      joystick (player 1)
    Z / X           fire buttons 1 and 2
    Enter           console Select
    Backspace       console Reset
    Delete          console Pause";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--cart" => self.cart = Some(args.path(flag)?),
            "--model" => {
                let id = args.value(flag)?;
                self.model = Model::from_variant_id(&id).ok_or_else(|| {
                    LaunchError::Usage(format!(
                        "unknown Atari 7800 model `{id}`; expected {}",
                        Model::VARIANT_IDS.join(", ")
                    ))
                })?;
            }
            "--region" => {
                self.model = match args.value(flag)?.as_str() {
                    "ntsc" => Model::A7800Ntsc,
                    "pal" => Model::A7800Pal,
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

    fn query_provider(&self) -> Atari7800SessionQueryProvider {
        Atari7800SessionQueryProvider
    }

    fn build_runtime(&self) -> Result<Atari7800Runtime, LaunchError> {
        if self.cart.is_none() {
            return Err(LaunchError::Run(
                "provide a cartridge with --cart PATH or as a positional argument".to_owned(),
            ));
        }
        self.build_mcp_runtime()
    }

    fn build_mcp_runtime(&self) -> Result<Atari7800Runtime, LaunchError> {
        let mut runtime = build_variant::<Atari7800Runtime>(self.model, &FirmwareOverrides::none())
            .map_err(|err| LaunchError::Run(err.to_string()))?;
        if let Some(path) = &self.cart {
            runtime
                .insert_cartridge(read_rom(path, "--cart")?)
                .map_err(|err| LaunchError::Run(err.to_string()))?;
        }
        Ok(runtime)
    }

    fn mcp_startup_media(
        &self,
        _slots: &[emu198x_shell::MediaSlot],
        _raw_args: &[String],
    ) -> Result<Vec<(String, emu198x_shell::MediaKind, Vec<u8>)>, LaunchError> {
        self.startup_media()
    }

    fn report(&self, runtime: &Atari7800Runtime, report: &mut Map<String, Value>) {
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

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn parse_cli_defaults() {
        let Parsed::Run { app, .. } = parse::<Atari7800>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.cart.is_none());
        assert_eq!(app.model, Model::A7800Ntsc);
    }

    #[test]
    fn parse_cli_accepts_cart_region_scale_video() {
        let parsed = parse::<Atari7800>(&args(&[
            "--cart", "game.a78", "--region", "pal", "--scale", "4", "--video", "crt",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.cart, Some(PathBuf::from("game.a78")));
        assert_eq!(app.model, Model::A7800Pal);
        assert_eq!(common.scale, Some(4));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn parse_cli_accepts_positional_cart() {
        let Parsed::Run { app, .. } = parse::<Atari7800>(&args(&["game.a78"])).expect("parses")
        else {
            panic!("expected a run");
        };
        assert_eq!(app.cart, Some(PathBuf::from("game.a78")));
    }

    #[test]
    fn a_bad_region_is_a_usage_error() {
        let err = parse::<Atari7800>(&args(&["--region", "secam"])).expect_err("rejects");
        assert_eq!(
            err,
            LaunchError::Usage("--region expects ntsc|pal, got secam".to_owned())
        );
    }

    #[test]
    fn region_frame_ticks_match() {
        assert_eq!(Model::A7800Ntsc.frame_ticks(), 262 * 228);
        assert_eq!(Model::A7800Pal.frame_ticks(), 312 * 228);
    }
}
