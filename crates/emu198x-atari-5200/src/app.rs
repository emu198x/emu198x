//! The Atari 5200 as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::PathBuf;

use emu198x_shell::launch::{Args, LaunchError, MachineApp, conventional_rom_path, read_rom};
use runtime_atari_5200::{Atari5200Runtime, Atari5200SessionQueryProvider, Model};
use serde_json::{Map, Value};

const BIOS_ENV: &str = "EMU198X_A5200_BIOS";
const BIOS_RELATIVE: &str = "atari-5200/bios.rom";

/// CPU clocks per frame — `lines × 228`.
const FRAME_TICKS_NTSC: u64 = 262 * 228;
#[cfg(feature = "ui")]
const NTSC_FRAME_HZ: f64 = 60.0;

/// Display region — selects the model, frame tick budget, and refresh rate.
///
/// The 5200 shipped in one television standard, so this names the only
/// one rather than offering a choice. See
/// `crates/runtime-atari-5200/src/profiles.rs` for the citation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Region {
    #[default]
    Ntsc,
}

impl Region {
    pub const fn model(self) -> Model {
        match self {
            Self::Ntsc => Model::A5200Ntsc,
        }
    }

    pub const fn frame_ticks(self) -> u64 {
        match self {
            Self::Ntsc => FRAME_TICKS_NTSC,
        }
    }

    #[cfg(feature = "ui")]
    pub const fn frame_hz(self) -> f64 {
        match self {
            Self::Ntsc => NTSC_FRAME_HZ,
        }
    }
}

/// The machine configuration the flags build up.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Atari5200 {
    /// `--cart PATH`, or the one positional argument.
    pub cart: Option<PathBuf>,
    pub bios: Option<PathBuf>,
    pub region: Region,
}

impl MachineApp for Atari5200 {
    type Runtime = Atari5200Runtime;
    type Query = Atari5200SessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-atari-5200";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --cart PATH     Atari 5200 cartridge ROM (required; a bare PATH is accepted too)
    --bios PATH     Atari 5200 BIOS ROM (2 KB); default
                    ~/.emu198x/roms/atari-5200/bios.rom or 5200.rom (or set EMU198X_A5200_BIOS)
    --region MODE   ntsc (the only standard the 5200 shipped in) [default: ntsc]";
    const CONTROLS: &'static str = "    Esc             quit
    F12             emulator hard reset
    Arrow keys      analogue stick (player 1)
    Z / X           fire
    Enter           Start    Backspace  Pause    Delete  Reset (keypad)
    0-9             keypad digits
    Numpad * / /    keypad * and # keys";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--cart" => self.cart = Some(args.path(flag)?),
            "--bios" => self.bios = Some(args.path(flag)?),
            "--region" => {
                self.region = match args.value(flag)?.as_str() {
                    "ntsc" => Region::Ntsc,
                    "pal" => {
                        return Err(LaunchError::Usage(
                            "the Atari 5200 shipped NTSC only — Atari's CX5200 Field Service \
                             Manual has a PAL GTIA in one as a part to replace, not a region \
                             to select"
                                .to_owned(),
                        ));
                    }
                    other => {
                        return Err(LaunchError::Usage(format!(
                            "--region expects ntsc, got {other}"
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

    fn query_provider(&self) -> Atari5200SessionQueryProvider {
        Atari5200SessionQueryProvider
    }

    fn build_runtime(&self) -> Result<Atari5200Runtime, LaunchError> {
        let Some(cart_path) = &self.cart else {
            return Err(LaunchError::Run(
                "provide a cartridge with --cart PATH".to_owned(),
            ));
        };
        let cart = read_rom(cart_path, "--cart")?;
        // The 5200 BIOS is optional — a best-effort read.
        let bios = self
            .bios
            .clone()
            .or_else(default_bios_path)
            .and_then(|path| std::fs::read(path).ok())
            .unwrap_or_default();
        if bios.is_empty() {
            eprintln!(
                "warning: no 5200 BIOS found (pass --bios PATH or stage bios.rom / \
                 5200.rom in ~/.emu198x/roms/atari-5200/); the screen will be blank \
                 without it."
            );
        }
        Atari5200Runtime::new(self.region.model(), cart, bios)
            .map_err(|err| LaunchError::Run(format!("failed to construct runtime: {err}")))
    }

    /// MCP starts blank — the cart arrives via load_media.
    fn build_mcp_runtime(&self) -> Result<Atari5200Runtime, LaunchError> {
        Ok(Atari5200Runtime::blank(self.region.model()))
    }

    fn report(&self, runtime: &Atari5200Runtime, report: &mut Map<String, Value>) {
        let cart_loaded = runtime.machine().is_some();
        let frame_count = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("cart_loaded".to_owned(), cart_loaded.into());
        report.insert("frames_run".to_owned(), frame_count.into());
    }
}

/// `$EMU198X_A5200_BIOS`, else the first of `bios.rom` / `5200.rom` that
/// exists in `~/.emu198x/roms/atari-5200/`.
///
/// The 5200 BIOS is staged under either name in the wild; return the first
/// that actually exists so a conventionally-named `5200.rom` is found (the
/// 5200 shows a black screen without its BIOS, so a constructed-but-missing
/// path is worse than useless).
fn default_bios_path() -> Option<PathBuf> {
    let bios = conventional_rom_path(BIOS_ENV, BIOS_RELATIVE)?;
    let alternate = bios.with_file_name("5200.rom");
    [bios, alternate].into_iter().find(|p| p.exists())
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
        let Parsed::Run { app, .. } = parse::<Atari5200>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.cart.is_none());
        assert_eq!(app.region, Region::Ntsc);
    }

    #[test]
    fn parse_cli_accepts_cart_bios_region_scale_video() {
        let parsed = parse::<Atari5200>(&args(&[
            "--cart", "game.a52", "--bios", "5200.rom", "--region", "ntsc", "--scale", "4",
            "--video", "crt",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.cart, Some(PathBuf::from("game.a52")));
        assert_eq!(app.bios, Some(PathBuf::from("5200.rom")));
        assert_eq!(app.region, Region::Ntsc);
        assert_eq!(common.scale, Some(4));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn parse_cli_accepts_positional_cart() {
        let Parsed::Run { app, .. } = parse::<Atari5200>(&args(&["game.a52"])).expect("parses")
        else {
            panic!("expected a run");
        };
        assert_eq!(app.cart, Some(PathBuf::from("game.a52")));
    }

    #[test]
    fn pal_is_refused_with_the_reason() {
        let err = parse::<Atari5200>(&args(&["--region", "pal"])).expect_err("rejects");
        assert!(matches!(err, LaunchError::Usage(msg) if msg.contains("NTSC only")));
    }

    #[test]
    fn region_frame_ticks_match() {
        assert_eq!(Region::Ntsc.frame_ticks(), 262 * 228);
    }
}
