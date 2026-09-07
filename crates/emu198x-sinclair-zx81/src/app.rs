//! The ZX81 as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::PathBuf;

use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom_exact, resolve_rom};
use runtime_sinclair_zx81::{Model, Zx81Runtime, Zx81SessionQueryProvider};
use serde_json::{Map, Value};

const ROM_ENV: &str = "EMU198X_ZX81_ROM";
const ROM_RELATIVE: &str = "sinclair-zx81/zx81.rom";
const ROM_SIZE: usize = 8 * 1024;

/// Frame budget for the board the runtime is configured as. The 60 Hz strap
/// lays out a much shorter field, so this cannot be one shared constant.
pub fn frame_ticks(model: Model) -> u64 {
    u64::from(model.television_standard().slow_mode_frame_tstates())
}

/// The machine configuration the flags build up.
#[derive(Debug, PartialEq, Eq)]
pub struct Zx81 {
    pub rom: Option<PathBuf>,
    pub ram_bytes: usize,
}

impl Default for Zx81 {
    fn default() -> Self {
        Self {
            rom: None,
            ram_bytes: 1024,
        }
    }
}

impl Zx81 {
    /// The board the binary starts on; the window can switch strap later.
    pub const fn model(&self) -> Model {
        Model::Zx81
    }
}

impl MachineApp for Zx81 {
    type Runtime = Zx81Runtime;
    type Query = Zx81SessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-sinclair-zx81";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str = "    --rom PATH      ZX81 monitor ROM (8 KB); default
                    ~/.emu198x/roms/sinclair-zx81/zx81.rom (or set EMU198X_ZX81_ROM)
    --ram-bytes N   RAM size (power-of-two ≤ 16384) [default: 1024]";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 . Space the ZX81 membrane keyboard
    Shift           SHIFT (the function/symbol layer — hold with another key)
    Enter           NEWLINE";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--rom" => self.rom = Some(args.path(flag)?),
            "--ram-bytes" => self.ram_bytes = args.parse(flag, "a positive integer")?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        frame_ticks(self.model())
    }

    fn query_provider(&self) -> Zx81SessionQueryProvider {
        Zx81SessionQueryProvider
    }

    fn build_runtime(&self) -> Result<Zx81Runtime, LaunchError> {
        let rom_path = resolve_rom(self.rom.as_deref(), ROM_ENV, ROM_RELATIVE)?;
        let rom = read_rom_exact(&rom_path, "ROM", ROM_SIZE)?;
        let mut runtime = Zx81Runtime::new(self.model(), rom)
            .map_err(|err| LaunchError::Run(format!("failed to construct runtime: {err}")))?;
        runtime
            .set_ram_bytes(self.ram_bytes)
            .map_err(|err| LaunchError::Run(format!("invalid --ram-bytes: {err}")))?;
        Ok(runtime)
    }

    /// MCP starts blank and takes the monitor ROM from its conventional
    /// location when an 8 KB image is there; a client can also hand it
    /// firmware later.
    fn build_mcp_runtime(&self) -> Result<Zx81Runtime, LaunchError> {
        let mut runtime = Zx81Runtime::blank(self.model());
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

    fn report(&self, runtime: &Zx81Runtime, report: &mut Map<String, Value>) {
        let frames_run = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("rom_loaded".to_owned(), runtime.machine().is_some().into());
        report.insert("frames_run".to_owned(), frames_run.into());
        report.insert("ram_bytes".to_owned(), self.ram_bytes.into());
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
        let Parsed::Run { app, .. } = parse::<Zx81>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert_eq!(app.ram_bytes, 1024);
    }

    #[test]
    fn parse_cli_accepts_rom_scale_video() {
        let parsed = parse::<Zx81>(&args(&[
            "--rom", "zx81.rom", "--scale", "4", "--video", "crt",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.rom, Some(PathBuf::from("zx81.rom")));
        assert_eq!(common.scale, Some(4));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }
}
