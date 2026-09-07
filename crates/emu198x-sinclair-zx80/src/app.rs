//! The ZX80 as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::PathBuf;

use emu198x_shell::MediaKind;
use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom, read_rom_exact, resolve_rom};
use runtime_sinclair_zx80::{Model, Zx80Runtime, Zx80SessionQueryProvider};
use serde_json::{Map, Value};

const ROM_ENV: &str = "EMU198X_ZX80_ROM";
const ROM_RELATIVE: &str = "sinclair-zx80/zx80.rom";
/// The ZX80's monitor ROM is 4 KB (half the ZX81's).
const ROM_SIZE: usize = 4 * 1024;

/// PAL TV-clock ticks per frame (207 per line × 312 lines).
pub const FRAME_TICKS_PAL: u64 = 207 * 312;

/// The machine configuration the flags build up.
#[derive(Debug, PartialEq, Eq)]
pub struct Zx80 {
    pub rom: Option<PathBuf>,
    pub ram_bytes: usize,
    /// `--tape PATH`: a .o/.80 cassette put in the deck before a script runs.
    pub tape: Option<PathBuf>,
}

impl Default for Zx80 {
    fn default() -> Self {
        Self {
            rom: None,
            ram_bytes: 1024,
            tape: None,
        }
    }
}

impl MachineApp for Zx80 {
    type Runtime = Zx80Runtime;
    type Query = Zx80SessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-sinclair-zx80";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str = "    --rom PATH      ZX80 monitor ROM (4 KB); default
                    ~/.emu198x/roms/sinclair-zx80/zx80.rom (or set EMU198X_ZX80_ROM)
    --ram-bytes N   RAM size (power-of-two ≤ 16384) [default: 1024]
    --tape PATH     put a .o/.80 cassette in the deck. This does not press
                    play: the loader's leader countdown is at the front of the
                    tape, so the script has to type LOAD (the W key) first, then
                    issue a `media_transport` start step on slot `tape-1`.";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 . Space the ZX80 membrane keyboard
    Shift           SHIFT (the function/symbol layer — hold with another key)
    Enter           NEWLINE";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--rom" => self.rom = Some(args.path(flag)?),
            "--ram-bytes" => self.ram_bytes = args.parse(flag, "a positive integer")?,
            "--tape" => self.tape = Some(args.path(flag)?),
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        FRAME_TICKS_PAL
    }

    fn query_provider(&self) -> Zx80SessionQueryProvider {
        Zx80SessionQueryProvider
    }

    fn build_runtime(&self) -> Result<Zx80Runtime, LaunchError> {
        let rom_path = resolve_rom(self.rom.as_deref(), ROM_ENV, ROM_RELATIVE)?;
        let rom = read_rom_exact(&rom_path, "ROM", ROM_SIZE)?;
        let mut runtime = Zx80Runtime::new(Model::Zx80, rom)
            .map_err(|err| LaunchError::Run(format!("failed to construct runtime: {err}")))?;
        runtime
            .set_ram_bytes(self.ram_bytes)
            .map_err(|err| LaunchError::Run(format!("invalid --ram-bytes: {err}")))?;
        Ok(runtime)
    }

    /// MCP starts blank and takes the monitor ROM from its conventional
    /// location when a 4 KB image is there; a client can also hand it
    /// firmware later.
    fn build_mcp_runtime(&self) -> Result<Zx80Runtime, LaunchError> {
        let mut runtime = Zx80Runtime::blank(Model::Zx80);
        if let Ok(path) = resolve_rom(self.rom.as_deref(), ROM_ENV, ROM_RELATIVE)
            && let Ok(bytes) = std::fs::read(&path)
        {
            if bytes.len() == ROM_SIZE {
                runtime
                    .set_rom(bytes)
                    .map_err(|err| LaunchError::Run(format!("ROM invalid: {err}")))?;
                eprintln!("{} mcp: loaded ROM from {}", Self::BIN_NAME, path.display());
            } else {
                eprintln!(
                    "{} mcp: ROM at {} is {} bytes; expected {ROM_SIZE} — starting blank",
                    Self::BIN_NAME,
                    path.display(),
                    bytes.len()
                );
            }
        }
        Ok(runtime)
    }

    fn startup_media(&self) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        let Some(path) = &self.tape else {
            return Ok(Vec::new());
        };
        let bytes = read_rom(path, "--tape")?;
        Ok(vec![("tape-1".to_owned(), MediaKind::Tape, bytes)])
    }

    fn report(&self, runtime: &Zx80Runtime, report: &mut Map<String, Value>) {
        let frames_run = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("rom_loaded".to_owned(), runtime.machine().is_some().into());
        report.insert("frames_run".to_owned(), frames_run.into());
        report.insert("ram_bytes".to_owned(), self.ram_bytes.into());
        report.insert("tape_loaded".to_owned(), self.tape.is_some().into());
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
        let Parsed::Run { app, .. } = parse::<Zx80>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert_eq!(app.ram_bytes, 1024);
    }

    #[test]
    fn parse_cli_accepts_rom_scale_video() {
        let parsed = parse::<Zx80>(&args(&[
            "--rom", "zx80.rom", "--scale", "4", "--video", "crt",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.rom, Some(PathBuf::from("zx80.rom")));
        assert_eq!(common.scale, Some(4));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn flags_set_ram_bytes_and_tape() {
        let parsed =
            parse::<Zx80>(&args(&["--ram-bytes", "16384", "--tape", "game.o"])).expect("parses");
        let Parsed::Run { app, .. } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.ram_bytes, 16384);
        assert_eq!(app.tape, Some(PathBuf::from("game.o")));
    }
}
