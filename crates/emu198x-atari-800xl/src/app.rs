//! The Atari 800XL as a [`MachineApp`]: its flags, runtime, and report fields.

use std::env;
use std::path::{Path, PathBuf};

use emu198x_shell::launch::{Args, LaunchError, MachineApp, conventional_rom_path, read_rom};
use emu198x_shell::{MediaKind, read_media_asset};
use runtime_atari_800xl::{Atari800xlRuntime, Atari800xlSessionQueryProvider, Model};
use serde_json::{Map, Value};

const OS_ENV: &str = "EMU198X_A800XL_OS";
const OS_RELATIVE: &str = "atari-800xl/atarixl.rom";
const BASIC_ENV: &str = "EMU198X_A800XL_BASIC";
const BASIC_RELATIVE: &str = "atari-800xl/ataribas.rom";

/// Colour clocks per frame — `lines × 228`. Chip timing (ANTIC/GTIA/POKEY)
/// matches the 5200 sibling: NTSC = 262 lines (~60 Hz), PAL = 312 (~50 Hz).
const FRAME_TICKS_NTSC: u64 = 262 * 228;
const FRAME_TICKS_PAL: u64 = 312 * 228;

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
            Self::Ntsc => Model::A800xlNtsc,
            Self::Pal => Model::A800xlPal,
        }
    }

    pub const fn frame_ticks(self) -> u64 {
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
#[derive(Debug, PartialEq, Eq)]
pub struct Atari800xl {
    pub os: Option<PathBuf>,
    pub basic: Option<PathBuf>,
    pub cart: Option<PathBuf>,
    /// `--disk PATH`: an ATR image for D1:, loaded before a script runs.
    pub disk: Option<PathBuf>,
    /// `--no-basic` clears this: OPTION is held at boot to disable BASIC.
    pub basic_enabled: bool,
    pub region: Region,
}

impl Default for Atari800xl {
    fn default() -> Self {
        Self {
            os: None,
            basic: None,
            cart: None,
            disk: None,
            basic_enabled: true,
            region: Region::Ntsc,
        }
    }
}

/// Where an optional firmware image is: `explicit`, else `$env_var` when
/// set, else the conventional file — but only if that file is there. The
/// OS and BASIC are both optional (a cartridge can boot on its own), so an
/// absent default is not an error; a path the user named is read
/// unconditionally and fails loudly.
fn optional_rom(explicit: Option<&Path>, env_var: &str, relative: &str) -> Option<PathBuf> {
    if let Some(path) = explicit {
        return Some(path.to_path_buf());
    }
    let path = conventional_rom_path(env_var, relative)?;
    let named_by_env = env::var(env_var).is_ok_and(|value| !value.is_empty());
    (named_by_env || path.exists()).then_some(path)
}

fn read_optional(path: Option<&Path>, flag: &str) -> Result<Option<Vec<u8>>, LaunchError> {
    path.map(|path| read_rom(path, flag)).transpose()
}

impl MachineApp for Atari800xl {
    type Runtime = Atari800xlRuntime;
    type Query = Atari800xlSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-atari-800xl";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str = "    --os PATH       16 KB OS ROM; default
                    ~/.emu198x/roms/atari-800xl/atarixl.rom (or EMU198X_A800XL_OS)
    --basic PATH    8 KB Atari BASIC ROM; default
                    ~/.emu198x/roms/atari-800xl/ataribas.rom (or EMU198X_A800XL_BASIC)
    --cart PATH     cartridge image (flat, XEGS, MegaCart or OSS; .car headers honoured)
    --disk PATH     ATR disk image for D1: (a .zip holding one .atr works too)
    --no-basic      hold OPTION at boot to disable the built-in BASIC
    --region MODE   ntsc | pal [default: ntsc]";
    const CONTROLS: &'static str = "    Esc             quit
    F12             emulator hard reset
    A-Z 0-9 etc.    the Atari keyboard
    Enter / Space / Delete / Tab   the matching Atari keys
    Arrow keys      joystick (player 1)
    F2 / F3 / F4    Start / Select / Option console keys
    Gamepad         joystick + fire (player 1)";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--os" => self.os = Some(args.path(flag)?),
            "--basic" => self.basic = Some(args.path(flag)?),
            "--cart" => self.cart = Some(args.path(flag)?),
            "--disk" => self.disk = Some(args.path(flag)?),
            "--no-basic" => self.basic_enabled = false,
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

    fn query_provider(&self) -> Atari800xlSessionQueryProvider {
        Atari800xlSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<Atari800xlRuntime, LaunchError> {
        let os_path = optional_rom(self.os.as_deref(), OS_ENV, OS_RELATIVE);
        let basic_path = optional_rom(self.basic.as_deref(), BASIC_ENV, BASIC_RELATIVE);
        let os = read_optional(os_path.as_deref(), "--os")?;
        let basic = read_optional(basic_path.as_deref(), "--basic")?;
        let cart = read_optional(self.cart.as_deref(), "--cart")?;

        if os.is_none() && cart.is_none() {
            return Err(LaunchError::Run(
                "either --os or --cart must be provided (cart-only boot uses the cart's reset vector)"
                    .to_owned(),
            ));
        }

        Atari800xlRuntime::new(self.region.model(), os, basic, cart, self.basic_enabled)
            .map_err(|err| LaunchError::Run(format!("failed to construct runtime: {err}")))
    }

    /// MCP starts blank — OS / BASIC / cart arrive via load_media.
    fn build_mcp_runtime(&self) -> Result<Atari800xlRuntime, LaunchError> {
        Ok(Atari800xlRuntime::blank(self.region.model()))
    }

    fn startup_media(&self) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        let Some(path) = &self.disk else {
            return Ok(Vec::new());
        };
        let loaded = read_media_asset(path, MediaKind::Disk).map_err(|err| {
            LaunchError::Run(format!(
                "failed to load disk asset {}: {err}",
                path.display()
            ))
        })?;
        Ok(vec![("disk-1".to_owned(), MediaKind::Disk, loaded.bytes)])
    }

    fn report(&self, runtime: &Atari800xlRuntime, report: &mut Map<String, Value>) {
        let machine_loaded = runtime.machine().is_some();
        let frames_run = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("machine_loaded".to_owned(), machine_loaded.into());
        report.insert("frames_run".to_owned(), frames_run.into());
        report.insert("basic_enabled".to_owned(), self.basic_enabled.into());
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
    fn defaults_to_ntsc_with_basic() {
        let parsed = parse::<Atari800xl>(&[]).expect("parses");
        let Parsed::Run { app, mode, .. } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.region, Region::Ntsc);
        assert!(app.basic_enabled);
        assert!(app.disk.is_none());
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn flags_set_roms_disk_basic_and_region() {
        let parsed = parse::<Atari800xl>(&args(&[
            "--cart",
            "game.bin",
            "--disk",
            "dos.atr",
            "--no-basic",
            "--region",
            "pal",
        ]))
        .expect("parses");
        let Parsed::Run { app, mode, .. } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.cart, Some(PathBuf::from("game.bin")));
        assert_eq!(app.disk, Some(PathBuf::from("dos.atr")));
        assert!(!app.basic_enabled);
        assert_eq!(app.region, Region::Pal);
        // A bare `--cart` is shared with the UI, so it opens the window.
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn a_bad_region_is_a_usage_error() {
        let err = parse::<Atari800xl>(&args(&["--region", "secam"])).expect_err("rejects");
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
