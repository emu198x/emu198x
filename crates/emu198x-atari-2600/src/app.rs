//! The Atari 2600 as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::{Path, PathBuf};

use emu198x_shell::launch::{Args, LaunchError, MachineApp};
use emu198x_shell::{FirmwareOverrides, build_variant};
use emu198x_shell::{MediaKind, read_media_asset};
use runtime_atari_2600::{Atari2600Runtime, Atari2600SessionQueryProvider, Model};
use serde_json::{Map, Value};

/// The machine configuration the flags build up.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Atari2600 {
    /// `--cart PATH`, or the one positional argument.
    pub cart: Option<PathBuf>,
    pub model: Model,
}

impl MachineApp for Atari2600 {
    type Runtime = Atari2600Runtime;
    type Query = Atari2600SessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-atari-2600";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --cart PATH     cartridge ROM (.a26/.bin, or a .zip). A multi-entry zip
                    (e.g. a merged MAME software list) loads its root parent;
                    append #NAME or #INDEX to pick another, e.g. game.zip#poleposc
                    (a bare PATH is accepted too)
    --model ID      atari-2600-ntsc | atari-2600-pal
    --region MODE   ntsc | pal [default: ntsc]";
    const CONTROLS: &'static str = "    Esc             quit
    F12             emulator hard reset
    Arrow keys      joystick (player 1)
    X / Z / Space   fire
    Enter           console RESET switch
    Right Shift     console SELECT switch";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--cart" => self.cart = Some(args.path(flag)?),
            "--model" => {
                let id = args.value(flag)?;
                self.model = Model::from_variant_id(&id).ok_or_else(|| {
                    LaunchError::Usage(format!(
                        "unknown Atari 2600 model `{id}`; expected {}",
                        Model::VARIANT_IDS.join(", ")
                    ))
                })?;
            }
            "--region" => {
                self.model = match args.value(flag)?.as_str() {
                    "ntsc" => Model::Vcs2600Ntsc,
                    "pal" => Model::Vcs2600Pal,
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

    fn query_provider(&self) -> Atari2600SessionQueryProvider {
        Atari2600SessionQueryProvider
    }

    fn build_runtime(&self) -> Result<Atari2600Runtime, LaunchError> {
        if self.cart.is_none() {
            return Err(LaunchError::Run(
                "provide a cartridge with --cart PATH or as a positional argument".to_owned(),
            ));
        }
        self.build_mcp_runtime()
    }

    fn build_mcp_runtime(&self) -> Result<Atari2600Runtime, LaunchError> {
        let mut runtime = build_variant::<Atari2600Runtime>(self.model, &FirmwareOverrides::none())
            .map_err(|err| LaunchError::Run(err.to_string()))?;
        if let Some(path) = &self.cart {
            runtime
                .insert_cartridge(load_cart_bytes(path)?)
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

    fn report(&self, runtime: &Atari2600Runtime, report: &mut Map<String, Value>) {
        let cart_loaded = runtime.machine().is_some();
        let frame_count = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("cart_loaded".to_owned(), cart_loaded.into());
        report.insert("frames_run".to_owned(), frame_count.into());
    }
}

fn load_cart_bytes(path: &Path) -> Result<Vec<u8>, LaunchError> {
    // Goes through the shell's media loader so a zipped cart (the TOSEC `.a26`
    // distribution form) is expanded transparently; a raw file is read as-is.
    read_media_asset(path, MediaKind::Cartridge)
        .map(|asset| asset.bytes)
        .map_err(|err| LaunchError::Run(format!("failed to read --cart {}: {err}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::launch::{CommonCli, Mode, Parsed, parse};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn run(list: &[&str]) -> (Atari2600, CommonCli, Mode) {
        match parse::<Atari2600>(&args(list)).expect("parses") {
            Parsed::Run { app, common, mode } => (app, common, mode),
            Parsed::Help => panic!("expected a run"),
        }
    }

    #[test]
    fn parse_cli_defaults() {
        let (app, _, _) = run(&[]);
        assert!(app.cart.is_none());
        assert_eq!(app.model, Model::Vcs2600Ntsc);
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let (app, common, mode) =
            run(&["--cart", "/tmp/cart", "--region", "pal", "--frames", "30"]);
        assert_eq!(app.cart.as_deref(), Some(Path::new("/tmp/cart")));
        assert_eq!(app.model, Model::Vcs2600Pal);
        assert_eq!(common.frames, 30);
        assert_eq!(mode, Mode::Script);
    }

    #[test]
    fn parse_cli_accepts_positional_cart_and_scale() {
        let (app, common, mode) = run(&["--scale", "2", "game.a26"]);
        assert_eq!(app.cart, Some(PathBuf::from("game.a26")));
        assert_eq!(common.scale, Some(2));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn a_second_positional_cart_is_a_usage_error() {
        let err = parse::<Atari2600>(&args(&["a.a26", "b.a26"])).expect_err("rejects");
        assert_eq!(
            err,
            LaunchError::Usage("only one positional cart path is supported".to_owned())
        );
    }

    #[test]
    fn a_bad_region_is_a_usage_error() {
        let err = parse::<Atari2600>(&args(&["--region", "secam"])).expect_err("rejects");
        assert_eq!(
            err,
            LaunchError::Usage("--region expects ntsc|pal, got secam".to_owned())
        );
    }

    #[test]
    fn region_frame_ticks_match() {
        assert_eq!(Model::Vcs2600Ntsc.frame_ticks(), 262 * 228);
        assert_eq!(Model::Vcs2600Pal.frame_ticks(), 312 * 228);
    }
}
