//! The Oric-1 / Atmos as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::PathBuf;

use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom_exact, resolve_rom};
use runtime_oric_atmos::{Model, OricRuntime, OricSessionQueryProvider};
use serde_json::{Map, Value};

const ROM_ENV: &str = "EMU198X_ORIC_ROM";
const ROM_RELATIVE: &str = "oric/oric.rom";
const ROM_SIZE: usize = 16 * 1024;

/// 6502 @ 1 MHz, 50 Hz PAL → 312 lines × 64 µs = 19,968 cycles/frame.
// Keep <= the machine's run_frame() size, or the harness runs two machine
// frames per displayed frame (~2x too fast). See docs/status/ui-boot-verification.
pub const FRAME_TICKS: u64 = 19_968;

/// The machine configuration the flags build up.
#[derive(Debug, PartialEq, Eq)]
pub struct Oric {
    pub rom: Option<PathBuf>,
    pub model: Model,
}

impl Default for Oric {
    fn default() -> Self {
        Self {
            rom: None,
            model: Model::Atmos,
        }
    }
}

impl MachineApp for Oric {
    type Runtime = OricRuntime;
    type Query = OricSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-oric-atmos";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str = "    --rom PATH      16 KB BASIC + OS ROM; default
                    ~/.emu198x/roms/oric/oric.rom (or set EMU198X_ORIC_ROM)
    --model NAME    oric-1 | atmos [default: atmos]";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the Oric keyboard (cursor keys are real Oric keys)
    Shift / Ctrl    the Oric shift / control keys
    Gamepad         IJK joystick (player 1, left stick)";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--rom" => self.rom = Some(args.path(flag)?),
            "--model" => {
                self.model = match args.value(flag)?.as_str() {
                    "oric-1" | "oric1" => Model::Oric1,
                    "atmos" => Model::Atmos,
                    other => {
                        return Err(LaunchError::Usage(format!(
                            "--model expects oric-1|atmos, got {other}"
                        )));
                    }
                };
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        FRAME_TICKS
    }

    fn query_provider(&self) -> OricSessionQueryProvider {
        OricSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<OricRuntime, LaunchError> {
        let rom_path = resolve_rom(self.rom.as_deref(), ROM_ENV, ROM_RELATIVE)?;
        let rom = read_rom_exact(&rom_path, "Oric ROM", ROM_SIZE)?;
        OricRuntime::new(self.model, rom)
            .map_err(|err| LaunchError::Run(format!("failed to construct runtime: {err}")))
    }

    /// MCP starts blank — the ROM arrives via firmware load.
    fn build_mcp_runtime(&self) -> Result<OricRuntime, LaunchError> {
        Ok(OricRuntime::blank(self.model))
    }

    fn report(&self, runtime: &OricRuntime, report: &mut Map<String, Value>) {
        let frames_run = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("rom_loaded".to_owned(), runtime.machine().is_some().into());
        report.insert("frames_run".to_owned(), frames_run.into());
        report.insert(
            "model".to_owned(),
            match self.model {
                Model::Oric1 => "oric-1",
                Model::Atmos => "atmos",
            }
            .into(),
        );
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
        let Parsed::Run { app, .. } = parse::<Oric>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert_eq!(app.model, Model::Atmos);
    }

    #[test]
    fn parse_cli_accepts_rom_model_scale_video() {
        let parsed = parse::<Oric>(&args(&[
            "--rom", "oric.rom", "--model", "oric-1", "--scale", "4", "--video", "crt",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.rom, Some(PathBuf::from("oric.rom")));
        assert_eq!(app.model, Model::Oric1);
        assert_eq!(common.scale, Some(4));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn a_bad_model_is_a_usage_error() {
        let err = parse::<Oric>(&args(&["--model", "telestrat"])).expect_err("rejects");
        assert_eq!(
            err,
            LaunchError::Usage("--model expects oric-1|atmos, got telestrat".to_owned())
        );
    }
}
