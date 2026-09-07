//! The MSX1 as a [`MachineApp`]: its flags, runtime, and report fields.
//!
//! MSX chip state — VDP, the AY-3-8910 PSG (`ay.*`), the 8255 PPI — is read
//! over MCP through the generic `query` tool as query paths (`vdp`, `ay`,
//! `ppi`, and their leaves), not bespoke tools; the one MSX-specific addition
//! to the tool set is the shared AY-watch verbs.

use std::path::PathBuf;

use emu198x_shell::HeadlessSession;
use emu198x_shell::launch::{
    Args, LaunchError, MachineApp, conventional_rom_path, read_rom, read_rom_exact,
};
use emu198x_shell::mcp::ToolRegistry;
use emu198x_shell::mcp_tools::{
    register_ay_watch_tools, register_base_tools, register_keyboard_tools,
};
use runtime_msx::{MapperType, Model, MsxRuntime, MsxSessionQueryProvider};
use serde_json::{Map, Value};

const BIOS_ENV: &str = "EMU198X_MSX_BIOS";
const BIOS_RELATIVE: &str = "microsoft-msx/msx.rom";
const BIOS_SIZE: usize = 32 * 1024;

/// CPU clocks per frame — `228 × lines`.
///
/// One MSX1 NTSC frame = 228 T-states × 262 scanlines.
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
            Self::Ntsc => Model::Msx1Ntsc,
            Self::Pal => Model::Msx1Pal,
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
#[derive(Debug, PartialEq, Eq)]
pub struct Msx {
    pub bios: Option<PathBuf>,
    pub cart: Option<PathBuf>,
    pub mapper: MapperType,
    pub cart2: Option<PathBuf>,
    pub mapper2: MapperType,
    pub region: Region,
}

impl Default for Msx {
    fn default() -> Self {
        Self {
            bios: None,
            cart: None,
            mapper: MapperType::Plain,
            cart2: None,
            mapper2: MapperType::Plain,
            region: Region::default(),
        }
    }
}

impl Msx {
    /// `--bios`, else `$EMU198X_MSX_BIOS`, else the conventional path.
    fn bios_path(&self) -> Result<PathBuf, LaunchError> {
        self.bios
            .clone()
            .or_else(|| conventional_rom_path(BIOS_ENV, BIOS_RELATIVE))
            .ok_or_else(|| {
                LaunchError::Run("no BIOS: pass --bios PATH or set EMU198X_MSX_BIOS".to_owned())
            })
    }
}

fn parse_mapper(flag: &str, value: &str) -> Result<MapperType, LaunchError> {
    Ok(match value {
        "plain" => MapperType::Plain,
        "konami" => MapperType::Konami,
        "konami-scc" => MapperType::KonamiScc,
        "ascii8" => MapperType::Ascii8,
        "ascii16" => MapperType::Ascii16,
        other => {
            return Err(LaunchError::Usage(format!(
                "{flag} expects plain|konami|konami-scc|ascii8|ascii16, got {other}"
            )));
        }
    })
}

impl MachineApp for Msx {
    type Runtime = MsxRuntime;
    type Query = MsxSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-msx";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str = "    --bios PATH     MSX1 BIOS ROM (32 KB); default
                    ~/.emu198x/roms/microsoft-msx/msx.rom (or set EMU198X_MSX_BIOS)
    --cart PATH     cartridge ROM (slot 1)
    --mapper KIND   cartridge mapper: plain | konami | konami-scc | ascii8 |
                    ascii16 [default: plain]
    --cart2 PATH    cartridge ROM (slot 2)
    --mapper2 KIND  slot-2 mapper [default: plain]
    --region MODE   ntsc | pal [default: ntsc]";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the MSX keyboard (cursor keys are real MSX keys)
    Shift / Ctrl    the MSX SHIFT / CTRL keys (Alt = GRAPH)
    Gamepad         joystick (player 1)";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--bios" => self.bios = Some(args.path(flag)?),
            "--cart" => self.cart = Some(args.path(flag)?),
            "--mapper" => self.mapper = parse_mapper(flag, &args.value(flag)?)?,
            "--cart2" => self.cart2 = Some(args.path(flag)?),
            "--mapper2" => self.mapper2 = parse_mapper(flag, &args.value(flag)?)?,
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

    fn query_provider(&self) -> MsxSessionQueryProvider {
        MsxSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<MsxRuntime, LaunchError> {
        let bios_path = self.bios_path()?;
        let bios = read_rom_exact(&bios_path, "BIOS", BIOS_SIZE)?;
        let mut runtime = MsxRuntime::new(self.region.model(), bios)
            .map_err(|err| LaunchError::Run(format!("failed to construct runtime: {err}")))?;
        // The cart slots are inserted directly into the machine; the bytes
        // don't need to round-trip through the session's MediaSet.
        if let Some(cart_path) = &self.cart {
            runtime.insert_cartridge1(read_rom(cart_path, "--cart")?, self.mapper);
        }
        if let Some(cart_path) = &self.cart2 {
            runtime.insert_cartridge2(read_rom(cart_path, "--cart2")?, self.mapper2);
        }
        Ok(runtime)
    }

    /// MCP starts blank and takes the BIOS from its conventional location
    /// when one is there and is the right size; otherwise the client can
    /// drive the machine against a snapshot or hand it firmware later.
    fn build_mcp_runtime(&self) -> Result<MsxRuntime, LaunchError> {
        let mut runtime = MsxRuntime::blank(self.region.model());
        if let Ok(path) = self.bios_path() {
            if let Ok(bytes) = std::fs::read(&path) {
                if bytes.len() == BIOS_SIZE {
                    runtime.set_bios(bytes).map_err(|err| {
                        LaunchError::Run(format!("BIOS at {} invalid: {err}", path.display()))
                    })?;
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
            } else {
                eprintln!(
                    "{} mcp: BIOS path {} not readable — starting blank",
                    Self::BIN_NAME,
                    path.display()
                );
            }
        }
        Ok(runtime)
    }

    fn report(&self, runtime: &MsxRuntime, report: &mut Map<String, Value>) {
        let bios_loaded = runtime.machine().is_some();
        let frame_count = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("bios_loaded".to_owned(), bios_loaded.into());
        report.insert(
            "cart1_loaded".to_owned(),
            (bios_loaded && self.cart.is_some()).into(),
        );
        report.insert(
            "cart2_loaded".to_owned(),
            (bios_loaded && self.cart2.is_some()).into(),
        );
        report.insert("frames_run".to_owned(), frame_count.into());
    }

    fn register_mcp_tools(
        &self,
        registry: &mut ToolRegistry<HeadlessSession<MsxRuntime, MsxSessionQueryProvider>>,
    ) {
        register_base_tools(registry);
        // The machine has a keyboard, so the shared press_key / type_string apply.
        register_keyboard_tools(registry);
        // The MSX carries an AY-3-8912 (PSG), so the shared AY-watch verbs apply.
        register_ay_watch_tools(registry);
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
        let Parsed::Run { app, common, .. } = parse::<Msx>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.bios.is_none());
        assert!(app.cart.is_none());
        assert!(matches!(app.mapper, MapperType::Plain));
        assert!(app.cart2.is_none());
        assert!(matches!(app.mapper2, MapperType::Plain));
        assert_eq!(app.region, Region::Ntsc);
        assert_eq!(common.frames, 0);
        assert!(common.script.is_none());
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let parsed = parse::<Msx>(&args(&[
            "--bios",
            "/tmp/msx.rom",
            "--cart",
            "/tmp/game.rom",
            "--mapper",
            "konami-scc",
            "--cart2",
            "/tmp/other.rom",
            "--mapper2",
            "ascii16",
            "--region",
            "pal",
            "--frames",
            "120",
            "--screenshot",
            "/tmp/shot.png",
            "--audio-capture",
            "/tmp/audio.wav",
            "--script",
            "/tmp/steps.json",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.bios.as_deref(), Some(Path::new("/tmp/msx.rom")));
        assert_eq!(app.cart.as_deref(), Some(Path::new("/tmp/game.rom")));
        assert!(matches!(app.mapper, MapperType::KonamiScc));
        assert_eq!(app.cart2.as_deref(), Some(Path::new("/tmp/other.rom")));
        assert!(matches!(app.mapper2, MapperType::Ascii16));
        assert_eq!(app.region, Region::Pal);
        assert_eq!(common.frames, 120);
        assert_eq!(
            common.screenshot.as_deref(),
            Some(Path::new("/tmp/shot.png"))
        );
        assert_eq!(
            common.audio_capture.as_deref(),
            Some(Path::new("/tmp/audio.wav"))
        );
        assert_eq!(common.script.as_deref(), Some(Path::new("/tmp/steps.json")));
        assert_eq!(mode, Mode::Script);
    }

    #[test]
    fn parse_cli_accepts_bios_cart_mapper_region_scale_video() {
        let parsed = parse::<Msx>(&args(&[
            "--bios",
            "msx.rom",
            "--cart",
            "nemesis.rom",
            "--mapper",
            "konami",
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
        assert_eq!(app.bios, Some(PathBuf::from("msx.rom")));
        assert_eq!(app.cart, Some(PathBuf::from("nemesis.rom")));
        assert_eq!(app.mapper, MapperType::Konami);
        assert_eq!(app.region, Region::Pal);
        assert_eq!(common.scale, Some(4));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn a_bad_mapper_names_its_flag() {
        let err = parse::<Msx>(&args(&["--mapper2", "megarom"])).expect_err("rejects");
        assert_eq!(
            err,
            LaunchError::Usage(
                "--mapper2 expects plain|konami|konami-scc|ascii8|ascii16, got megarom".to_owned()
            )
        );
    }

    #[test]
    fn region_frame_ticks_match() {
        assert_eq!(Region::Ntsc.frame_ticks(), 228 * 262);
        assert_eq!(Region::Pal.frame_ticks(), 228 * 313);
    }
}
