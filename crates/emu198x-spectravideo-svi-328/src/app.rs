//! The Spectravideo SVI-328 as a [`MachineApp`]: its flags, runtime, and
//! report fields.

use std::path::PathBuf;

use emu198x_shell::launch::{
    Args, LaunchError, MachineApp, conventional_rom_path, read_rom, read_rom_exact,
};
use runtime_spectravideo_svi_328::{Model, Svi328Runtime, Svi328SessionQueryProvider};
use serde_json::{Map, Value};

const BIOS_ENV: &str = "EMU198X_SVI_328_BIOS";
const BIOS_RELATIVE: &str = "spectravideo-svi-328/svi-328.rom";
const BIOS_SIZE: usize = 32 * 1024;

/// CPU clocks per frame — `228 × lines`.
///
/// Z80 @ 3.58 MHz, 60 Hz NTSC → 228 * 262 = ~59,736 t-states/frame.
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
            Self::Ntsc => Model::Svi328Ntsc,
            Self::Pal => Model::Svi328Pal,
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
pub struct Svi328 {
    pub bios: Option<PathBuf>,
    pub cart: Option<PathBuf>,
    pub region: Region,
}

impl MachineApp for Svi328 {
    type Runtime = Svi328Runtime;
    type Query = Svi328SessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-spectravideo-svi-328";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --bios PATH     32 KB system ROM (BASIC + OS); default
                    ~/.emu198x/roms/spectravideo-svi-328/svi-328.rom
                    (or set EMU198X_SVI_328_BIOS)
    --cart PATH     cartridge ROM (up to 16 KB at $8000-$BFFF)
    --region MODE   ntsc | pal [default: ntsc]";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the SVI-328 keyboard (cursor keys are real SVI keys)
    Shift / Ctrl    the SVI SHIFT / CTRL keys (Alt = GRAPH/CODE)
    Gamepad         joystick (player 1)";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--bios" => self.bios = Some(args.path(flag)?),
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
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        self.region.frame_ticks()
    }

    fn query_provider(&self) -> Svi328SessionQueryProvider {
        Svi328SessionQueryProvider
    }

    fn build_runtime(&self) -> Result<Svi328Runtime, LaunchError> {
        let bios_path = self
            .bios
            .clone()
            .or_else(|| conventional_rom_path(BIOS_ENV, BIOS_RELATIVE))
            .ok_or_else(|| {
                LaunchError::Run("no BIOS: pass --bios PATH or set EMU198X_SVI_328_BIOS".to_owned())
            })?;
        let bios = read_rom_exact(&bios_path, "BIOS", BIOS_SIZE)?;
        let mut runtime = Svi328Runtime::new(self.region.model(), bios)
            .map_err(|err| LaunchError::Run(format!("failed to construct runtime: {err}")))?;
        if let Some(cart_path) = &self.cart {
            runtime
                .insert_cartridge(read_rom(cart_path, "--cart")?)
                .map_err(|err| LaunchError::Run(format!("failed to insert cart: {err}")))?;
        }
        Ok(runtime)
    }

    /// MCP starts blank — the ROM arrives via firmware load.
    fn build_mcp_runtime(&self) -> Result<Svi328Runtime, LaunchError> {
        Ok(Svi328Runtime::blank(self.region.model()))
    }

    fn report(&self, runtime: &Svi328Runtime, report: &mut Map<String, Value>) {
        let bios_loaded = runtime.machine().is_some();
        let frame_count = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("bios_loaded".to_owned(), bios_loaded.into());
        report.insert("cart_loaded".to_owned(), self.cart.is_some().into());
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
        let Parsed::Run { app, .. } = parse::<Svi328>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.bios.is_none());
        assert!(app.cart.is_none());
        assert_eq!(app.region, Region::Ntsc);
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
        assert_eq!(app.bios, Some(PathBuf::from("svi.rom")));
        assert_eq!(app.cart, Some(PathBuf::from("game.rom")));
        assert_eq!(app.region, Region::Pal);
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
        assert_eq!(Region::Ntsc.frame_ticks(), 228 * 262);
        assert_eq!(Region::Pal.frame_ticks(), 228 * 313);
    }
}
