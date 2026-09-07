//! The Acorn Electron as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::{Path, PathBuf};

use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom_exact, resolve_rom};
use runtime_acorn_electron::{ElectronRuntime, ElectronSessionQueryProvider, Model};
use serde_json::{Map, Value};

const OS_ENV: &str = "EMU198X_ELECTRON_OS";
const OS_RELATIVE: &str = "acorn-electron/os.rom";
const BASIC_ENV: &str = "EMU198X_ELECTRON_BASIC";
const BASIC_RELATIVE: &str = "acorn-electron/basic.rom";
const ROM_SIZE: usize = 16 * 1024;

/// 6502A @ 2 MHz nominal, 50 Hz → 40,000 cycles/frame nominal; the machine's
/// own frame is 312 lines × 128 cycles = 39,936.
// Keep <= the machine's run_frame() size, or the harness runs two machine
// frames per displayed frame (~2x too fast). See docs/status/ui-boot-verification.
pub const FRAME_TICKS_PAL: u64 = 39_936;

/// The machine configuration the flags build up.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Electron {
    pub os: Option<PathBuf>,
    pub basic: Option<PathBuf>,
}

/// The path of one of the two firmware images, naming its own flag and
/// variable when neither is available.
fn firmware_path(
    explicit: Option<&Path>,
    flag: &str,
    kind: &str,
    env_var: &str,
    relative: &str,
) -> Result<PathBuf, LaunchError> {
    resolve_rom(explicit, env_var, relative)
        .map_err(|_| LaunchError::Run(format!("no {kind} ROM: pass {flag} PATH or set {env_var}")))
}

impl MachineApp for Electron {
    type Runtime = ElectronRuntime;
    type Query = ElectronSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-acorn-electron";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str = "    --os PATH       Electron OS ROM (16 KB); default
                    ~/.emu198x/roms/acorn-electron/os.rom (or set EMU198X_ELECTRON_OS)
    --basic PATH    BBC BASIC II ROM (16 KB); default
                    ~/.emu198x/roms/acorn-electron/basic.rom
                    (or set EMU198X_ELECTRON_BASIC)";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the Electron keyboard (cursor keys are real Electron keys)
    Shift / Ctrl    the Electron SHIFT / CTRL keys; Alt = FUNC; End = COPY";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--os" => self.os = Some(args.path(flag)?),
            "--basic" => self.basic = Some(args.path(flag)?),
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        FRAME_TICKS_PAL
    }

    fn query_provider(&self) -> ElectronSessionQueryProvider {
        ElectronSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<ElectronRuntime, LaunchError> {
        let os_path = firmware_path(self.os.as_deref(), "--os", "OS", OS_ENV, OS_RELATIVE)?;
        let basic_path = firmware_path(
            self.basic.as_deref(),
            "--basic",
            "BASIC",
            BASIC_ENV,
            BASIC_RELATIVE,
        )?;
        let os = read_rom_exact(&os_path, "OS ROM", ROM_SIZE)?;
        let basic = read_rom_exact(&basic_path, "BASIC ROM", ROM_SIZE)?;
        ElectronRuntime::new(Model::Electron, os, basic)
            .map_err(|err| LaunchError::Run(format!("failed to construct runtime: {err}")))
    }

    /// MCP starts blank and takes OS + BASIC from their conventional
    /// locations when both 16 KB images are there; a client can also hand it
    /// firmware later.
    fn build_mcp_runtime(&self) -> Result<ElectronRuntime, LaunchError> {
        let mut runtime = ElectronRuntime::blank(Model::Electron);
        if let (Ok(os_path), Ok(basic_path)) = (
            resolve_rom(self.os.as_deref(), OS_ENV, OS_RELATIVE),
            resolve_rom(self.basic.as_deref(), BASIC_ENV, BASIC_RELATIVE),
        ) && let (Ok(os), Ok(basic)) = (std::fs::read(&os_path), std::fs::read(&basic_path))
        {
            if os.len() == ROM_SIZE && basic.len() == ROM_SIZE {
                runtime
                    .set_roms(os, basic)
                    .map_err(|err| LaunchError::Run(format!("ROM invalid: {err}")))?;
                eprintln!(
                    "{} mcp: loaded OS={} BASIC={}",
                    Self::BIN_NAME,
                    os_path.display(),
                    basic_path.display(),
                );
            } else {
                eprintln!(
                    "{} mcp: ROM sizes wrong (OS={} bytes, BASIC={} bytes) — starting blank",
                    Self::BIN_NAME,
                    os.len(),
                    basic.len()
                );
            }
        }
        Ok(runtime)
    }

    fn report(&self, runtime: &ElectronRuntime, report: &mut Map<String, Value>) {
        let frames_run = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("roms_loaded".to_owned(), runtime.machine().is_some().into());
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
        let Parsed::Run { app, .. } = parse::<Electron>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.os.is_none());
        assert!(app.basic.is_none());
    }

    #[test]
    fn parse_cli_accepts_full_flags() {
        let parsed = parse::<Electron>(&args(&[
            "--os",
            "/tmp/os",
            "--basic",
            "/tmp/basic",
            "--frames",
            "30",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.os.expect("parsed by CLI"), Path::new("/tmp/os"));
        assert_eq!(app.basic.expect("parsed by CLI"), Path::new("/tmp/basic"));
        assert_eq!(common.frames, 30);
        assert_eq!(mode, Mode::Script);
    }

    #[test]
    fn parse_cli_accepts_os_basic_scale_video() {
        let parsed = parse::<Electron>(&args(&[
            "--os",
            "os.rom",
            "--basic",
            "basic.rom",
            "--scale",
            "2",
            "--video",
            "crt",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.os, Some(PathBuf::from("os.rom")));
        assert_eq!(app.basic, Some(PathBuf::from("basic.rom")));
        assert_eq!(common.scale, Some(2));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }
}
