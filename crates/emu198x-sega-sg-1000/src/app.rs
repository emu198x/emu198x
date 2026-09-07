//! The Sega SG-1000 as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::PathBuf;

use emu198x_shell::HeadlessSession;
use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom};
use emu198x_shell::mcp::ToolRegistry;
use emu198x_shell::mcp_tools::register_base_tools;
use runtime_sega_sg_1000::{Model, Sg1000Runtime, Sg1000SessionQueryProvider};
use serde_json::{Map, Value};

/// CPU clocks per frame — `228 × lines`.
const FRAME_TICKS_NTSC: u64 = 228 * 262;
const FRAME_TICKS_PAL: u64 = 228 * 313;
#[cfg(feature = "ui")]
const NTSC_FRAME_HZ: f64 = 60.0;
#[cfg(feature = "ui")]
const PAL_FRAME_HZ: f64 = 50.0;

/// Display region — selects the model, frame tick budget, and refresh rate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Region {
    #[default]
    Ntsc,
    Pal,
}

impl Region {
    pub const fn model(self) -> Model {
        match self {
            Self::Ntsc => Model::Sg1000Ntsc,
            Self::Pal => Model::Sg1000Pal,
        }
    }

    pub const fn frame_ticks(self) -> u64 {
        match self {
            Self::Ntsc => FRAME_TICKS_NTSC,
            Self::Pal => FRAME_TICKS_PAL,
        }
    }

    #[cfg(feature = "ui")]
    pub const fn frame_hz(self) -> f64 {
        match self {
            Self::Ntsc => NTSC_FRAME_HZ,
            Self::Pal => PAL_FRAME_HZ,
        }
    }
}

/// The machine configuration the flags build up.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Sg1000 {
    /// `--cart PATH`, or the one positional argument.
    pub cart: Option<PathBuf>,
    pub region: Region,
}

impl MachineApp for Sg1000 {
    type Runtime = Sg1000Runtime;
    type Query = Sg1000SessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-sega-sg-1000";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --cart PATH     cartridge ROM (required; a bare PATH is accepted too)
    --region MODE   ntsc | pal [default: ntsc]";
    const CONTROLS: &'static str = "    Esc             quit
    F12             emulator hard reset
    Arrow keys      d-pad (player 1)
    Z / X           buttons 1 and 2
    Enter           Pause";

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

    fn query_provider(&self) -> Sg1000SessionQueryProvider {
        Sg1000SessionQueryProvider
    }

    fn build_runtime(&self) -> Result<Sg1000Runtime, LaunchError> {
        let Some(cart_path) = &self.cart else {
            return Err(LaunchError::Run(
                "provide a cartridge with --cart PATH".to_owned(),
            ));
        };
        let cart = read_rom(cart_path, "--cart")?;
        Ok(Sg1000Runtime::new(self.region.model(), cart))
    }

    /// MCP starts blank; the cartridge arrives via load_media.
    fn build_mcp_runtime(&self) -> Result<Sg1000Runtime, LaunchError> {
        Ok(Sg1000Runtime::blank(self.region.model()))
    }

    fn report(&self, runtime: &Sg1000Runtime, report: &mut Map<String, Value>) {
        let cart_loaded = runtime.machine().is_some();
        let frame_count = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("cart_loaded".to_owned(), cart_loaded.into());
        report.insert("frames_run".to_owned(), frame_count.into());
    }

    /// The SG-1000 has no keyboard: the base tools only.
    fn register_mcp_tools(
        &self,
        registry: &mut ToolRegistry<HeadlessSession<Sg1000Runtime, Sg1000SessionQueryProvider>>,
    ) {
        register_base_tools(registry);
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
        assert_eq!(app.region, Region::Ntsc);
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
        assert_eq!(app.region, Region::Pal);
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
        assert_eq!(app.region, Region::Pal);
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
        assert_eq!(app.region, Region::Ntsc);
    }

    #[test]
    fn region_frame_ticks_match() {
        assert_eq!(Region::Ntsc.frame_ticks(), 228 * 262);
        assert_eq!(Region::Pal.frame_ticks(), 228 * 313);
    }
}
