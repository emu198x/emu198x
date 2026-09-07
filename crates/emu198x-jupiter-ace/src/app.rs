//! The Jupiter Ace as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::PathBuf;

use emu198x_shell::MediaKind;
use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom, read_rom_exact, resolve_rom};
use runtime_jupiter_ace::{JupiterAceRuntime, JupiterAceSessionQueryProvider, Model};
use serde_json::{Map, Value};

const ROM_ENV: &str = "EMU198X_JUPITER_ACE_ROM";
const ROM_RELATIVE: &str = "jupiter-ace/ace.rom";
const ROM_SIZE: usize = 8192;

/// Z80 @ 3.25 MHz PAL: 312 lines x 208 T-states = 64,896 T-states/frame.
/// Derived from the machine so it cannot drift: a budget longer than
/// run_frame() makes the harness run two machine frames per displayed frame
/// (~2x too fast). See docs/status/ui-boot-verification.
pub const FRAME_TICKS: u64 = machine_jupiter_ace::TSTATES_PER_FRAME as u64;

/// The machine configuration the flags build up.
#[derive(Debug, PartialEq, Eq)]
pub struct JupiterAce {
    pub rom: Option<PathBuf>,
    pub ram_kb: usize,
    /// `--ace PATH`: an ACE32 snapshot restored before a script runs.
    pub ace: Option<PathBuf>,
}

impl Default for JupiterAce {
    fn default() -> Self {
        Self {
            rom: None,
            ram_kb: 3,
            ace: None,
        }
    }
}

pub fn model_for(ram_kb: usize) -> Model {
    match ram_kb {
        n if n >= 48 => Model::Ace48k,
        n if n >= 16 => Model::Ace16k,
        _ => Model::Ace3k,
    }
}

impl MachineApp for JupiterAce {
    type Runtime = JupiterAceRuntime;
    type Query = JupiterAceSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-jupiter-ace";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --rom PATH      Jupiter Ace Forth ROM (8 KB); default
                    ~/.emu198x/roms/jupiter-ace/ace.rom
                    (or set EMU198X_JUPITER_ACE_ROM)
    --ram-kb N      base RAM in KB (3 / 16 / 48) [default: 3]
    --ace PATH      restore an ACE32 .ace snapshot before running";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 Space   the Ace keyboard
    Shift           CAPS SHIFT (hold with another key)
    Ctrl            SYMBOL SHIFT (the red symbol layer)
    Enter           ENTER";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--rom" => self.rom = Some(args.path(flag)?),
            "--ram-kb" => self.ram_kb = args.parse(flag, "a non-negative integer")?,
            "--ace" => self.ace = Some(args.path(flag)?),
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        FRAME_TICKS
    }

    fn query_provider(&self) -> JupiterAceSessionQueryProvider {
        JupiterAceSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<JupiterAceRuntime, LaunchError> {
        let rom_path = resolve_rom(self.rom.as_deref(), ROM_ENV, ROM_RELATIVE)?;
        let rom = read_rom_exact(&rom_path, "Forth ROM", ROM_SIZE)?;
        JupiterAceRuntime::new(model_for(self.ram_kb), rom)
            .map_err(|err| LaunchError::Run(format!("failed to construct runtime: {err}")))
    }

    /// MCP starts blank — the ROM arrives via firmware load.
    fn build_mcp_runtime(&self) -> Result<JupiterAceRuntime, LaunchError> {
        Ok(JupiterAceRuntime::blank(model_for(self.ram_kb)))
    }

    fn startup_media(&self) -> Result<Vec<(String, MediaKind, Vec<u8>)>, LaunchError> {
        let Some(path) = &self.ace else {
            return Ok(Vec::new());
        };
        let bytes = read_rom(path, "--ace")?;
        Ok(vec![("snapshot-1".to_owned(), MediaKind::Snapshot, bytes)])
    }

    fn report(&self, runtime: &JupiterAceRuntime, report: &mut Map<String, Value>) {
        let frames_run = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("rom_loaded".to_owned(), runtime.machine().is_some().into());
        report.insert("frames_run".to_owned(), frames_run.into());
        report.insert("ram_kb".to_owned(), self.ram_kb.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::HeadlessSession;
    use emu198x_shell::launch::{Parsed, parse};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn flags_set_rom_and_ram() {
        let parsed =
            parse::<JupiterAce>(&args(&["--rom", "ace.rom", "--ram-kb", "16"])).expect("parses");
        let Parsed::Run { app, .. } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.rom, Some(PathBuf::from("ace.rom")));
        assert_eq!(app.ram_kb, 16);
    }

    #[test]
    fn model_selects_by_ram() {
        assert_eq!(model_for(3), Model::Ace3k);
        assert_eq!(model_for(16), Model::Ace16k);
        assert_eq!(model_for(48), Model::Ace48k);
    }

    #[test]
    fn native_budget_runs_exact_requested_frame_count() {
        let runtime =
            JupiterAceRuntime::new(Model::Ace3k, vec![0; 8 * 1024]).expect("valid test ROM");
        let mut session = HeadlessSession::new(runtime, FRAME_TICKS);

        session.run_frames(1).expect("first frame");
        assert_eq!(
            session.machine().machine().expect("machine").frame_count(),
            1
        );

        session.run_frames(3).expect("three more frames");
        assert_eq!(
            session.machine().machine().expect("machine").frame_count(),
            4
        );
    }
}
