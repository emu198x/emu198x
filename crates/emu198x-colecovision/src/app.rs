//! The ColecoVision as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::PathBuf;

use emu198x_shell::HeadlessSession;
use emu198x_shell::launch::{
    Args, LaunchError, MachineApp, conventional_rom_path, read_rom, read_rom_exact,
};
use emu198x_shell::mcp::ToolRegistry;
use emu198x_shell::mcp_tools::register_base_tools;
use runtime_coleco_colecovision::{CvRuntime, CvSessionQueryProvider, Model};
use serde_json::{Map, Value};

const BIOS_ENV: &str = "EMU198X_COLECO_BIOS";
const BIOS_RELATIVE: &str = "coleco-colecovision/colecovision.rom";
const BIOS_SIZE: usize = 8 * 1024;

/// CPU clocks per frame — `228 × lines`.
///
/// ColecoVision NTSC: 228 T-states × 262 scanlines.
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
            Self::Ntsc => Model::CvNtsc,
            Self::Pal => Model::CvPal,
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
pub struct ColecoVision {
    pub bios: Option<PathBuf>,
    /// `--cart PATH`, or the one positional argument.
    pub cart: Option<PathBuf>,
    pub region: Region,
}

impl ColecoVision {
    /// `--bios`, else `$EMU198X_COLECO_BIOS`, else the conventional path.
    fn bios_path(&self) -> Result<PathBuf, LaunchError> {
        self.bios
            .clone()
            .or_else(|| conventional_rom_path(BIOS_ENV, BIOS_RELATIVE))
            .ok_or_else(|| {
                LaunchError::Run("no BIOS: pass --bios PATH or set EMU198X_COLECO_BIOS".to_owned())
            })
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
    --cart PATH     cartridge ROM image (optional — BIOS shows the splash;
                    a bare PATH is accepted too)
    --region MODE   ntsc | pal [default: ntsc]";
    const CONTROLS: &'static str = "    Esc             quit
    F12             emulator hard reset
    Arrow keys      joystick (player 1)
    Z / X           left and right fire buttons
    0-9             numeric keypad
    Numpad * / /    keypad * and # keys";

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

    fn query_provider(&self) -> CvSessionQueryProvider {
        CvSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<CvRuntime, LaunchError> {
        let bios_path = self.bios_path()?;
        let bios = read_rom_exact(&bios_path, "BIOS", BIOS_SIZE)?;
        let mut runtime = CvRuntime::new(self.region.model(), bios)
            .map_err(|err| LaunchError::Run(format!("failed to construct runtime: {err}")))?;
        if let Some(cart_path) = &self.cart {
            runtime.insert_cartridge(read_rom(cart_path, "--cart")?);
        }
        Ok(runtime)
    }

    /// MCP starts blank and takes the BIOS from its conventional location
    /// when one is there and is the right size; a client can also hand it
    /// firmware later.
    fn build_mcp_runtime(&self) -> Result<CvRuntime, LaunchError> {
        let mut runtime = CvRuntime::blank(self.region.model());
        if let Ok(path) = self.bios_path()
            && let Ok(bytes) = std::fs::read(&path)
        {
            if bytes.len() == BIOS_SIZE {
                runtime
                    .set_bios(bytes)
                    .map_err(|err| LaunchError::Run(format!("BIOS invalid: {err}")))?;
                eprintln!(
                    "{} mcp: loaded BIOS from {}",
                    Self::BIN_NAME,
                    path.display()
                );
            } else {
                eprintln!(
                    "{} mcp: BIOS at {} is {} bytes; expected {BIOS_SIZE} — starting blank",
                    Self::BIN_NAME,
                    path.display(),
                    bytes.len()
                );
            }
        }
        Ok(runtime)
    }

    fn report(&self, runtime: &CvRuntime, report: &mut Map<String, Value>) {
        let bios_loaded = runtime.machine().is_some();
        let frame_count = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("bios_loaded".to_owned(), bios_loaded.into());
        report.insert(
            "cart_loaded".to_owned(),
            (bios_loaded && self.cart.is_some()).into(),
        );
        report.insert("frames_run".to_owned(), frame_count.into());
    }

    /// The ColecoVision has a keypad, not a keyboard: the base tools only.
    fn register_mcp_tools(
        &self,
        registry: &mut ToolRegistry<HeadlessSession<CvRuntime, CvSessionQueryProvider>>,
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
        let Parsed::Run { app, common, .. } = parse::<ColecoVision>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.bios.is_none());
        assert!(app.cart.is_none());
        assert_eq!(app.region, Region::Ntsc);
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
        assert_eq!(app.bios.as_deref(), Some(Path::new("/tmp/bios")));
        assert_eq!(app.cart.as_deref(), Some(Path::new("/tmp/cart")));
        assert_eq!(app.region, Region::Pal);
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
        assert_eq!(app.bios, Some(PathBuf::from("coleco.rom")));
        assert_eq!(app.cart, Some(PathBuf::from("dk.col")));
        assert_eq!(app.region, Region::Pal);
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
    fn region_frame_ticks_match() {
        assert_eq!(Region::Ntsc.frame_ticks(), 228 * 262);
        assert_eq!(Region::Pal.frame_ticks(), 228 * 313);
    }
}
