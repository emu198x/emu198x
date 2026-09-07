//! The Atari 2600 as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::{Path, PathBuf};

use emu198x_shell::launch::{Args, LaunchError, MachineApp};
use emu198x_shell::mcp::ToolRegistry;
use emu198x_shell::mcp_tools::register_base_tools;
use emu198x_shell::{HeadlessSession, MediaKind, read_media_asset};
use runtime_atari_2600::{Atari2600Runtime, Atari2600SessionQueryProvider, Model};
use serde_json::{Map, Value};

/// Atari 2600 NTSC frame = 262 lines × 228 colour clocks.
const FRAME_TICKS_NTSC: u64 = 262 * 228;
const FRAME_TICKS_PAL: u64 = 312 * 228;

/// Display region — selects the model and the frame tick budget.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Region {
    #[default]
    Ntsc,
    Pal,
}

impl Region {
    const fn model(self) -> Model {
        match self {
            Self::Ntsc => Model::Vcs2600Ntsc,
            Self::Pal => Model::Vcs2600Pal,
        }
    }

    const fn frame_ticks(self) -> u64 {
        match self {
            Self::Ntsc => FRAME_TICKS_NTSC,
            Self::Pal => FRAME_TICKS_PAL,
        }
    }
}

/// The machine configuration the flags build up.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Atari2600 {
    /// `--cart PATH`, or the one positional argument.
    pub cart: Option<PathBuf>,
    pub region: Region,
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
            "--region" => {
                self.region = match args.value(flag)?.as_str() {
                    "ntsc" => Region::Ntsc,
                    "pal" => Region::Pal,
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
        self.region.frame_ticks()
    }

    fn query_provider(&self) -> Atari2600SessionQueryProvider {
        Atari2600SessionQueryProvider
    }

    fn build_runtime(&self) -> Result<Atari2600Runtime, LaunchError> {
        let Some(cart_path) = &self.cart else {
            return Err(LaunchError::Run(
                "provide a cartridge with --cart PATH or as a positional argument".to_owned(),
            ));
        };
        let cart = load_cart_bytes(cart_path)?;
        Atari2600Runtime::new(self.region.model(), cart)
            .map_err(|err| LaunchError::Run(format!("failed to construct runtime: {err}")))
    }

    /// MCP starts blank; the cartridge arrives via load_media.
    fn build_mcp_runtime(&self) -> Result<Atari2600Runtime, LaunchError> {
        Ok(Atari2600Runtime::blank(self.region.model()))
    }

    fn report(&self, runtime: &Atari2600Runtime, report: &mut Map<String, Value>) {
        let cart_loaded = runtime.machine().is_some();
        let frame_count = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("cart_loaded".to_owned(), cart_loaded.into());
        report.insert("frames_run".to_owned(), frame_count.into());
    }

    /// The 2600 has no keyboard: the base tools only.
    fn register_mcp_tools(
        &self,
        registry: &mut ToolRegistry<
            HeadlessSession<Atari2600Runtime, Atari2600SessionQueryProvider>,
        >,
    ) {
        register_base_tools(registry);
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
        assert_eq!(app.region, Region::Ntsc);
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let (app, common, mode) =
            run(&["--cart", "/tmp/cart", "--region", "pal", "--frames", "30"]);
        assert_eq!(app.cart.as_deref(), Some(Path::new("/tmp/cart")));
        assert_eq!(app.region, Region::Pal);
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
        assert_eq!(Region::Ntsc.frame_ticks(), 262 * 228);
        assert_eq!(Region::Pal.frame_ticks(), 312 * 228);
    }
}
