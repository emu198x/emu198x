//! The BBC Micro as a [`MachineApp`]: its flags, runtime, and report fields.

use std::fs;
use std::path::PathBuf;

use emu198x_shell::launch::{
    Args, LaunchError, MachineApp, conventional_rom_path, read_rom_exact, resolve_rom,
};
use runtime_acorn_bbc_micro::{BbcMicroRuntime, BbcMicroSessionQueryProvider, Model};
use serde_json::{Map, Value};

const MOS_ENV: &str = "EMU198X_BBC_MOS";
/// The headless modes' conventional MOS image. The window looks for
/// `mos.rom` instead; see [`crate::ui`]. Aligning the two is a separate fix.
const MOS_RELATIVE: &str = "acorn-bbc-micro/os.rom";
const MOS_SIZE: usize = 16 * 1024;
const FONT_ENV: &str = "EMU198X_BBC_SAA5050";
const FONT_RELATIVE: &str = "acorn-bbc-micro/saa5050.rom";

/// 6502 @ 2 MHz, 50 Hz → 40,000 cycles/frame nominal; the machine's own
/// frame is 312 lines × 128 cycles = 39,936.
// Keep <= the machine's run_frame() size, or the harness runs two machine
// frames per displayed frame (~2x too fast). See docs/status/ui-boot-verification.
pub const FRAME_TICKS_PAL: u64 = 39_936;

/// The machine configuration the flags build up.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Bbc {
    pub mos: Option<PathBuf>,
    /// `--sideways BANK=PATH`, repeatable: ROMs to install in banks 0..=15.
    pub sideways: Vec<(usize, PathBuf)>,
}

impl Bbc {
    /// The MOS image: `--mos`, else `$EMU198X_BBC_MOS`, else `relative`
    /// under the ROM directory.
    pub(crate) fn read_mos(&self, relative: &str) -> Result<Vec<u8>, LaunchError> {
        let mos_path = resolve_rom(self.mos.as_deref(), MOS_ENV, relative).map_err(|_| {
            LaunchError::Run("no MOS ROM: pass --mos PATH or set EMU198X_BBC_MOS".to_owned())
        })?;
        read_rom_exact(&mos_path, "MOS ROM", MOS_SIZE)
    }

    /// Install the `--sideways` banks.
    pub(crate) fn insert_sideways_roms(
        &self,
        runtime: &mut BbcMicroRuntime,
    ) -> Result<(), LaunchError> {
        for (bank, path) in &self.sideways {
            let rom = fs::read(path).map_err(|err| {
                LaunchError::Run(format!(
                    "failed to read sideways ROM bank {bank} {}: {err}",
                    path.display()
                ))
            })?;
            runtime.insert_sideways_rom(*bank, rom);
        }
        Ok(())
    }

    /// A machine on `mos`, without any language ROM yet.
    pub(crate) fn new_runtime(mos: Vec<u8>) -> Result<BbcMicroRuntime, LaunchError> {
        BbcMicroRuntime::new(Model::BbcModelB, mos)
            .map_err(|err| LaunchError::Run(format!("failed to construct runtime: {err}")))
    }
}

/// The path of an optional ROM: `$env_var` when set, else the conventional
/// file when it exists.
pub(crate) fn optional_rom_path(env_var: &str, relative: &str) -> Option<PathBuf> {
    let path = conventional_rom_path(env_var, relative)?;
    if std::env::var(env_var).is_ok_and(|value| !value.is_empty()) || path.exists() {
        Some(path)
    } else {
        None
    }
}

/// Load the SAA5050 teletext character ROM (MODE 7) if one is available.
pub(crate) fn load_teletext_font(runtime: &mut BbcMicroRuntime) {
    if let Some(font_path) = optional_rom_path(FONT_ENV, FONT_RELATIVE)
        && let Ok(font) = fs::read(&font_path)
    {
        runtime.set_teletext_font(font);
    }
}

fn parse_sideways(spec: &str) -> Result<(usize, PathBuf), LaunchError> {
    let Some((bank_str, path_str)) = spec.split_once('=') else {
        return Err(LaunchError::Usage(
            "--sideways expects BANK=PATH".to_owned(),
        ));
    };
    let bank: usize = bank_str
        .parse()
        .map_err(|_| LaunchError::Usage("--sideways bank must be an integer 0..=15".to_owned()))?;
    if bank > 15 {
        return Err(LaunchError::Usage(
            "--sideways bank must be 0..=15".to_owned(),
        ));
    }
    Ok((bank, PathBuf::from(path_str)))
}

impl MachineApp for Bbc {
    type Runtime = BbcMicroRuntime;
    type Query = BbcMicroSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-acorn-bbc-micro";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str = "    --mos PATH      BBC MOS ROM (16 KB); default
                    ~/.emu198x/roms/acorn-bbc-micro/os.rom (or set EMU198X_BBC_MOS)
    --sideways BANK=PATH    install a sideways ROM into bank 0..=15 (repeatable);
                    headless runs boot the bare MOS unless one is given, the
                    window also installs a staged BASIC into bank 15";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the BBC keyboard (cursor keys are real BBC keys)
    Shift / Ctrl    the BBC SHIFT / CTRL keys; F1-F10 are the red f0-f9 keys
    Gamepad         joystick fire (player 1)";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--mos" => self.mos = Some(args.path(flag)?),
            "--sideways" => self.sideways.push(parse_sideways(&args.value(flag)?)?),
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        FRAME_TICKS_PAL
    }

    fn query_provider(&self) -> BbcMicroSessionQueryProvider {
        BbcMicroSessionQueryProvider
    }

    /// The bare MOS with any `--sideways` banks: headless callers name
    /// their language ROM explicitly.
    fn build_runtime(&self) -> Result<BbcMicroRuntime, LaunchError> {
        let mut runtime = Self::new_runtime(self.read_mos(MOS_RELATIVE)?)?;
        self.insert_sideways_roms(&mut runtime)?;
        load_teletext_font(&mut runtime);
        Ok(runtime)
    }

    /// MCP starts blank and takes the MOS from its conventional location
    /// when a 16 KB image is there; a client can also hand it firmware later.
    fn build_mcp_runtime(&self) -> Result<BbcMicroRuntime, LaunchError> {
        let mut runtime = BbcMicroRuntime::blank(Model::BbcModelB);
        if let Ok(path) = resolve_rom(self.mos.as_deref(), MOS_ENV, MOS_RELATIVE)
            && let Ok(bytes) = fs::read(&path)
        {
            if bytes.len() == MOS_SIZE {
                runtime
                    .set_mos(bytes)
                    .map_err(|err| LaunchError::Run(format!("MOS invalid: {err}")))?;
                eprintln!("{} mcp: loaded MOS from {}", Self::BIN_NAME, path.display());
            } else {
                eprintln!(
                    "{} mcp: MOS at {} is {} bytes; expected {MOS_SIZE} — starting blank",
                    Self::BIN_NAME,
                    path.display(),
                    bytes.len()
                );
            }
        }
        Ok(runtime)
    }

    fn report(&self, runtime: &BbcMicroRuntime, report: &mut Map<String, Value>) {
        let frames_run = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("mos_loaded".to_owned(), runtime.machine().is_some().into());
        report.insert("sideways_count".to_owned(), self.sideways.len().into());
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
    fn parse_cli_defaults() {
        let Parsed::Run { app, .. } = parse::<Bbc>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.mos.is_none());
        assert!(app.sideways.is_empty());
    }

    #[test]
    fn parse_cli_accepts_mos_scale_video() {
        let parsed = parse::<Bbc>(&args(&[
            "--mos", "mos.rom", "--scale", "2", "--video", "crt",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.mos, Some(PathBuf::from("mos.rom")));
        assert_eq!(common.scale, Some(2));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn sideways_is_repeatable_and_bank_checked() {
        let Parsed::Run { app, .. } = parse::<Bbc>(&args(&[
            "--sideways",
            "15=basic.rom",
            "--sideways",
            "14=dfs.rom",
        ]))
        .expect("parses") else {
            panic!("expected a run");
        };
        assert_eq!(
            app.sideways,
            vec![
                (15, PathBuf::from("basic.rom")),
                (14, PathBuf::from("dfs.rom"))
            ]
        );

        let usage = |list: &[&str]| parse::<Bbc>(&args(list)).expect_err("rejects");
        assert_eq!(
            usage(&["--sideways", "basic.rom"]),
            LaunchError::Usage("--sideways expects BANK=PATH".to_owned())
        );
        assert_eq!(
            usage(&["--sideways", "x=basic.rom"]),
            LaunchError::Usage("--sideways bank must be an integer 0..=15".to_owned())
        );
        assert_eq!(
            usage(&["--sideways", "16=basic.rom"]),
            LaunchError::Usage("--sideways bank must be 0..=15".to_owned())
        );
    }
}
