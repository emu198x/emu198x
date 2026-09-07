//! The Sord M5 as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::PathBuf;

use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom, resolve_rom};
use runtime_sord_m5::{M5Runtime, M5SessionQueryProvider, Model};
use serde_json::{Map, Value};

const ROM_ENV: &str = "EMU198X_SORD_M5_ROM";
const ROM_RELATIVE: &str = "sord-m5/sord-m5.rom";

/// CPU clocks per frame — `228 × lines`.
const FRAME_TICKS_NTSC: u64 = 228 * 262;
const FRAME_TICKS_PAL: u64 = 228 * 313;

/// Display region — selects the model, frame tick budget, and refresh rate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Region {
    #[default]
    Ntsc,
    Pal,
}

impl Region {
    pub fn model(self) -> Model {
        match self {
            Self::Ntsc => Model::M5Ntsc,
            Self::Pal => Model::M5Pal,
        }
    }

    pub fn frame_ticks(self) -> u64 {
        match self {
            Self::Ntsc => FRAME_TICKS_NTSC,
            Self::Pal => FRAME_TICKS_PAL,
        }
    }

    #[cfg(feature = "ui")]
    pub fn frame_hz(self) -> f64 {
        match self {
            Self::Ntsc => 60.0,
            Self::Pal => 50.0,
        }
    }
}

/// The machine configuration the flags build up.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SordM5 {
    pub rom: Option<PathBuf>,
    pub cart: Option<PathBuf>,
    pub region: Region,
}

impl MachineApp for SordM5 {
    type Runtime = M5Runtime;
    type Query = M5SessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-sord-m5";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --rom PATH      Sord M5 BIOS ROM (monitor + BASIC-I); default
                    ~/.emu198x/roms/sord-m5/sord-m5.rom (or set EMU198X_SORD_M5_ROM)
    --cart PATH     cartridge ROM (optional)
    --region MODE   ntsc | pal [default: ntsc]";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the M5 keyboard
    Shift / Ctrl    the M5 SHIFT / CONTROL keys (Tab = FUNC)
    Gamepad         joystick (player 1, directions)";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--rom" => self.rom = Some(args.path(flag)?),
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

    fn query_provider(&self) -> M5SessionQueryProvider {
        M5SessionQueryProvider
    }

    fn build_runtime(&self) -> Result<M5Runtime, LaunchError> {
        let rom_path = resolve_rom(self.rom.as_deref(), ROM_ENV, ROM_RELATIVE)?;
        let rom = read_rom(&rom_path, "ROM")?;
        let mut runtime = M5Runtime::new(self.region.model(), rom);
        if let Some(cart_path) = &self.cart {
            runtime.insert_cartridge(read_rom(cart_path, "--cart")?);
        }
        Ok(runtime)
    }

    /// MCP starts blank and takes the BIOS from its conventional location
    /// when one is there; a client can also hand it firmware later.
    fn build_mcp_runtime(&self) -> Result<M5Runtime, LaunchError> {
        let mut runtime = M5Runtime::blank(self.region.model());
        if let Ok(path) = resolve_rom(self.rom.as_deref(), ROM_ENV, ROM_RELATIVE)
            && let Ok(bytes) = std::fs::read(&path)
        {
            runtime.set_rom(bytes);
            eprintln!("{} mcp: loaded ROM from {}", Self::BIN_NAME, path.display());
        }
        Ok(runtime)
    }

    fn report(&self, runtime: &M5Runtime, report: &mut Map<String, Value>) {
        let rom_loaded = runtime.machine().is_some();
        let frames_run = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("rom_loaded".to_owned(), rom_loaded.into());
        report.insert(
            "cart_loaded".to_owned(),
            (rom_loaded && self.cart.is_some()).into(),
        );
        report.insert("frames_run".to_owned(), frames_run.into());
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
    fn flags_set_rom_cart_and_region() {
        let parsed = parse::<SordM5>(&args(&[
            "--rom", "m5.rom", "--cart", "game.rom", "--region", "pal", "--scale", "4",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.rom, Some(PathBuf::from("m5.rom")));
        assert_eq!(app.cart, Some(PathBuf::from("game.rom")));
        assert_eq!(app.region, Region::Pal);
        assert_eq!(common.scale, Some(4));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn a_bad_region_is_a_usage_error() {
        let err = parse::<SordM5>(&args(&["--region", "secam"])).expect_err("rejects");
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
