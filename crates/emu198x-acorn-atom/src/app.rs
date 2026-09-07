//! The Acorn Atom as a [`MachineApp`]: its flags, runtime, and report fields.

use std::fs;
use std::path::PathBuf;

use emu198x_shell::HeadlessSession;
use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom_exact, resolve_rom};
use runtime_acorn_atom::{AtomRuntime, AtomSessionQueryProvider, Model};
use serde_json::{Map, Value};

const ROM_ENV: &str = "EMU198X_ACORN_ATOM_ROM";
const ROM_RELATIVE: &str = "acorn-atom/atom.rom";
const ROM_SIZE: usize = 24 * 1024;

// Atom: 6502 @ 1 MHz, 50 Hz PAL → ~20,000 cycles/frame.
pub const FRAME_TICKS: u64 = 20_000;

/// The machine configuration the flags build up.
#[derive(Debug, PartialEq, Eq)]
pub struct Atom {
    pub rom: Option<PathBuf>,
    pub ram_kb: usize,
    /// `--save-tape PATH`: write any cassette SAVE captured during the run
    /// as a .uef.
    pub save_tape: Option<PathBuf>,
    /// `--save-print PATH`: write any bytes sent to the Centronics printer.
    pub save_print: Option<PathBuf>,
}

impl Default for Atom {
    fn default() -> Self {
        Self {
            rom: None,
            ram_kb: 2,
            save_tape: None,
            save_print: None,
        }
    }
}

pub fn model_for(ram_kb: usize) -> Model {
    if ram_kb >= 12 {
        Model::AtomFull
    } else {
        Model::AtomBase
    }
}

impl MachineApp for Atom {
    type Runtime = AtomRuntime;
    type Query = AtomSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-acorn-atom";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --rom PATH      24 KB combined ROM (BASIC1 + FP + BASIC2 + OS); default
                    ~/.emu198x/roms/acorn-atom/atom.rom
                    (or set EMU198X_ACORN_ATOM_ROM)
    --ram-kb N      base RAM in KB (~2, or >=12 for a fully-expanded 32K) [default: 2]
    --save-tape PATH        write any cassette SAVE captured during a headless
                            run as a .uef
    --save-print PATH       write any bytes sent to the Centronics printer
                            during a headless run";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the Atom keyboard
    Enter           RETURN";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--rom" => self.rom = Some(args.path(flag)?),
            "--ram-kb" => self.ram_kb = args.parse(flag, "a non-negative integer")?,
            "--save-tape" => self.save_tape = Some(args.path(flag)?),
            "--save-print" => self.save_print = Some(args.path(flag)?),
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        FRAME_TICKS
    }

    fn query_provider(&self) -> AtomSessionQueryProvider {
        AtomSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<AtomRuntime, LaunchError> {
        let rom_path = resolve_rom(self.rom.as_deref(), ROM_ENV, ROM_RELATIVE)?;
        let rom = read_rom_exact(&rom_path, "Atom ROM", ROM_SIZE)?;
        AtomRuntime::new(model_for(self.ram_kb), rom)
            .map_err(|err| LaunchError::Run(format!("failed to construct runtime: {err}")))
    }

    /// MCP starts blank — the ROM arrives via firmware load.
    fn build_mcp_runtime(&self) -> Result<AtomRuntime, LaunchError> {
        Ok(AtomRuntime::blank(model_for(self.ram_kb)))
    }

    /// `--save-tape` and `--save-print` are captures too: with nothing to
    /// run they would write an empty file that reads as a success.
    fn requests_capture(&self) -> bool {
        self.save_tape.is_some() || self.save_print.is_some()
    }

    /// Write what the machine sent to the cassette and the printer during
    /// the run.
    fn after_run(
        &self,
        session: &mut HeadlessSession<AtomRuntime, AtomSessionQueryProvider>,
    ) -> Result<(), LaunchError> {
        if let Some(path) = &self.save_tape {
            let uef = session.machine_mut().flush_tape_image().ok_or_else(|| {
                LaunchError::Run("--save-tape: no cassette SAVE was captured".to_owned())
            })?;
            fs::write(path, &uef).map_err(|err| {
                LaunchError::Run(format!("failed to write {}: {err}", path.display()))
            })?;
        }
        if let Some(path) = &self.save_print {
            let bytes = session
                .machine_mut()
                .flush_printer_output()
                .ok_or_else(|| {
                    LaunchError::Run("--save-print: nothing was sent to the printer".to_owned())
                })?;
            fs::write(path, &bytes).map_err(|err| {
                LaunchError::Run(format!("failed to write {}: {err}", path.display()))
            })?;
        }
        Ok(())
    }

    fn report(&self, runtime: &AtomRuntime, report: &mut Map<String, Value>) {
        let frames_run = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("rom_loaded".to_owned(), runtime.machine().is_some().into());
        report.insert("frames_run".to_owned(), frames_run.into());
        report.insert("ram_kb".to_owned(), self.ram_kb.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::launch::{CommonCli, Mode, Parsed, parse, script_report};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn parse_cli_defaults() {
        let Parsed::Run { app, .. } = parse::<Atom>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert_eq!(app.ram_kb, 2);
        assert!(app.save_tape.is_none());
        assert!(app.save_print.is_none());
    }

    #[test]
    fn parse_cli_accepts_rom_ram_scale_video() {
        let parsed = parse::<Atom>(&args(&[
            "--rom", "atom.rom", "--ram-kb", "12", "--scale", "4", "--video", "crt",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.rom, Some(PathBuf::from("atom.rom")));
        assert_eq!(app.ram_kb, 12);
        assert_eq!(common.scale, Some(4));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn save_flags_are_captures() {
        let Parsed::Run { app, .. } = parse::<Atom>(&args(&[
            "--save-tape",
            "out.uef",
            "--save-print",
            "out.txt",
        ]))
        .expect("parses") else {
            panic!("expected a run");
        };
        assert_eq!(app.save_tape, Some(PathBuf::from("out.uef")));
        assert_eq!(app.save_print, Some(PathBuf::from("out.txt")));
        assert!(app.requests_capture());
        assert!(!Atom::default().requests_capture());
    }

    #[test]
    fn a_save_with_nothing_to_run_is_refused() {
        // Otherwise the .uef is empty and reads as a success. The launcher
        // refuses before it reads the ROM, so no ROM is needed.
        let app = Atom {
            save_tape: Some(PathBuf::from("out.uef")),
            ..Atom::default()
        };
        let err = script_report(&app, &CommonCli::default()).expect_err("should refuse");
        assert!(
            err.to_string().contains("capture requests require"),
            "{err}"
        );
    }

    #[test]
    fn model_selects_by_ram() {
        assert_eq!(model_for(2), Model::AtomBase);
        assert_eq!(model_for(12), Model::AtomFull);
    }
}
