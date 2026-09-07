//! The Mattel Aquarius as a [`MachineApp`]: its flags, runtime, and report
//! fields.

use std::path::PathBuf;

use emu198x_shell::HeadlessSession;
use emu198x_shell::launch::{
    Args, LaunchError, MachineApp, conventional_rom_path, read_rom, read_rom_exact,
};
use emu198x_shell::mcp::ToolRegistry;
use emu198x_shell::mcp_tools::{
    register_ay_watch_tools, register_base_tools, register_keyboard_tools,
};
use runtime_mattel_aquarius::{AquariusRuntime, AquariusSessionQueryProvider, Model};
use serde_json::{Map, Value};

const BIOS_ENV: &str = "EMU198X_AQUARIUS_BIOS";
const BIOS_RELATIVE: &str = "mattel-aquarius/aquarius.rom";
const BIOS_SIZE: usize = 8 * 1024;
const CHAR_ENV: &str = "EMU198X_AQUARIUS_CHAR";
const CHAR_RELATIVE: &str = "mattel-aquarius/aquarius-char.rom";

/// Aquarius runs at ~3.58 MHz CPU; PAL frame = ~71,569 T-states.
///
/// Z80 @ ~3.58 MHz, ~50 Hz PAL → 71,590 t-states/frame.
pub const FRAME_TICKS_PAL: u64 = 71_590;

/// The machine configuration the flags build up.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Aquarius {
    pub bios: Option<PathBuf>,
    pub char_rom: Option<PathBuf>,
    pub cart: Option<PathBuf>,
    pub expansion_kb: usize,
}

impl Aquarius {
    /// `--bios`, else `$EMU198X_AQUARIUS_BIOS`, else the conventional path.
    fn bios_path(&self) -> Result<PathBuf, LaunchError> {
        self.bios
            .clone()
            .or_else(|| conventional_rom_path(BIOS_ENV, BIOS_RELATIVE))
            .ok_or_else(|| {
                LaunchError::Run(
                    "no BIOS: pass --bios PATH or set EMU198X_AQUARIUS_BIOS".to_owned(),
                )
            })
    }

    /// `--char`, else `$EMU198X_AQUARIUS_CHAR`, else the conventional path.
    fn char_path(&self) -> Result<PathBuf, LaunchError> {
        self.char_rom
            .clone()
            .or_else(|| conventional_rom_path(CHAR_ENV, CHAR_RELATIVE))
            .ok_or_else(|| {
                LaunchError::Run(
                    "no character ROM: pass --char PATH or set EMU198X_AQUARIUS_CHAR".to_owned(),
                )
            })
    }
}

impl MachineApp for Aquarius {
    type Runtime = AquariusRuntime;
    type Query = AquariusSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-mattel-aquarius";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str = "    --bios PATH     Aquarius BASIC ROM (8 KB); default
                    ~/.emu198x/roms/mattel-aquarius/aquarius.rom
                    (or set EMU198X_AQUARIUS_BIOS)
    --char PATH     Aquarius character ROM (2 KB); default
                    ~/.emu198x/roms/mattel-aquarius/aquarius-char.rom
                    (or set EMU198X_AQUARIUS_CHAR)
    --cart PATH     cartridge ROM (mapped at $E000-$FFFF, up to 8 KB)
    --expansion-kb N
                    RAM expansion in KB (0..=16) [default: 0]";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the Aquarius keyboard
    Shift / Ctrl    the two Aquarius shift keys
    Arrow keys      hand controller disc (player 1)
    Alt             hand controller fire";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--bios" => self.bios = Some(args.path(flag)?),
            "--char" => self.char_rom = Some(args.path(flag)?),
            "--cart" => self.cart = Some(args.path(flag)?),
            "--expansion-kb" => self.expansion_kb = args.parse(flag, "a non-negative integer")?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        FRAME_TICKS_PAL
    }

    fn query_provider(&self) -> AquariusSessionQueryProvider {
        AquariusSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<AquariusRuntime, LaunchError> {
        let bios_path = self.bios_path()?;
        let bios = read_rom_exact(&bios_path, "BIOS", BIOS_SIZE)?;
        let mut runtime = AquariusRuntime::new(Model::Aquarius, bios)
            .map_err(|err| LaunchError::Run(format!("failed to construct runtime: {err}")))?;
        // The 2 KB character-generator ROM is separate from the BASIC ROM;
        // without it the display is garbage. Default to the standard install
        // path.
        let char_path = self.char_path()?;
        let char_rom = read_rom(&char_path, "character ROM")?;
        runtime
            .set_char_rom(char_rom)
            .map_err(|err| LaunchError::Run(format!("character ROM rejected: {err}")))?;
        runtime.set_expansion_kb(self.expansion_kb);
        if let Some(cart_path) = &self.cart {
            runtime.insert_cartridge(read_rom(cart_path, "--cart")?);
        }
        Ok(runtime)
    }

    /// MCP starts blank and takes the BIOS from its conventional location
    /// when one is there and is the right size; a client can also hand it
    /// firmware later.
    fn build_mcp_runtime(&self) -> Result<AquariusRuntime, LaunchError> {
        let mut runtime = AquariusRuntime::blank(Model::Aquarius);
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

    fn report(&self, runtime: &AquariusRuntime, report: &mut Map<String, Value>) {
        let bios_loaded = runtime.machine().is_some();
        let frame_count = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("bios_loaded".to_owned(), bios_loaded.into());
        report.insert(
            "cart_loaded".to_owned(),
            (bios_loaded && self.cart.is_some()).into(),
        );
        report.insert("frames_run".to_owned(), frame_count.into());
        report.insert("expansion_kb".to_owned(), self.expansion_kb.into());
    }

    fn register_mcp_tools(
        &self,
        registry: &mut ToolRegistry<HeadlessSession<AquariusRuntime, AquariusSessionQueryProvider>>,
    ) {
        register_base_tools(registry);
        // The machine has a keyboard, so the shared press_key / type_string apply.
        register_keyboard_tools(registry);
        // The Aquarius (Mini Expander) carries an AY-3-8910 (PSG), so the AY-watch verbs apply.
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
        let Parsed::Run { app, .. } = parse::<Aquarius>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.bios.is_none());
        assert!(app.char_rom.is_none());
        assert!(app.cart.is_none());
        assert_eq!(app.expansion_kb, 0);
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let parsed = parse::<Aquarius>(&args(&[
            "--bios",
            "/tmp/aq.rom",
            "--cart",
            "/tmp/game",
            "--expansion-kb",
            "16",
            "--frames",
            "60",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.bios.as_deref(), Some(Path::new("/tmp/aq.rom")));
        assert_eq!(app.expansion_kb, 16);
        assert_eq!(common.frames, 60);
        assert_eq!(mode, Mode::Script);
    }

    #[test]
    fn parse_cli_accepts_bios_char_cart_expansion_scale_video() {
        let parsed = parse::<Aquarius>(&args(&[
            "--bios",
            "aq.rom",
            "--char",
            "aq-char.rom",
            "--cart",
            "game.bin",
            "--expansion-kb",
            "16",
            "--scale",
            "4",
            "--video",
            "crt",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.bios, Some(PathBuf::from("aq.rom")));
        assert_eq!(app.char_rom, Some(PathBuf::from("aq-char.rom")));
        assert_eq!(app.cart, Some(PathBuf::from("game.bin")));
        assert_eq!(app.expansion_kb, 16);
        assert_eq!(common.scale, Some(4));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn a_bad_expansion_is_a_usage_error() {
        let err = parse::<Aquarius>(&args(&["--expansion-kb", "lots"])).expect_err("rejects");
        assert_eq!(
            err,
            LaunchError::Usage(
                "--expansion-kb expects a non-negative integer, got lots".to_owned()
            )
        );
    }
}
