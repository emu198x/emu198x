//! The Memotech MTX as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::{Path, PathBuf};

use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom, resolve_rom};
use runtime_memotech_mtx::{Model, MtxRuntime, MtxSessionQueryProvider};
use serde_json::{Map, Value};

const ROM_ENV: &str = "EMU198X_MTX_ROM";
const ROM_RELATIVE: &str = "memotech-mtx/mtx.rom";
/// 8 KB OS plus paged ROMs: any multiple of 8 KB, at least 16 KB.
const MIN_ROM_SIZE: usize = 16 * 1024;
const ROM_PAGE: usize = 0x2000;

// The frame-granular runtime always finishes the current frame, so a budget
// longer than one frame crosses the next boundary and emits two frames per
// MCP call. The old 80,000-tick nominal (4 MHz / 50 Hz) did exactly that.
//
// The VDP is clocked from the CPU through a rational accumulator, so the PAL
// frame alternates 79,747 and 79,746 T-states -- 79,746.5 on average, with no
// exact integer period. Take the floor: every real frame is then at least as
// long as the budget, so `run_frames(n)` never overshoots, and the half-tick
// shortfall only costs a frame once it accumulates past one full frame
// (n > 159,492, roughly 53 minutes of emulated time).
pub const FRAME_TICKS_PAL: u64 = 79_746;

/// The machine configuration the flags build up.
#[derive(Debug, PartialEq, Eq)]
pub struct Mtx {
    pub rom: Option<PathBuf>,
    pub model: Model,
}

impl Default for Mtx {
    fn default() -> Self {
        Self {
            rom: None,
            model: Model::Mtx500,
        }
    }
}

/// True when `len` is the 8 KB OS plus whole 8 KB paged ROMs.
fn rom_size_is_valid(len: usize) -> bool {
    len >= MIN_ROM_SIZE && len.is_multiple_of(ROM_PAGE)
}

fn read_mtx_rom(path: &Path) -> Result<Vec<u8>, LaunchError> {
    let bytes = read_rom(path, "ROM")?;
    if !rom_size_is_valid(bytes.len()) {
        return Err(LaunchError::Run(format!(
            "ROM at {} is {} bytes; expected the 8 KB OS plus 8 KB paged ROMs \
             (a multiple of 8192, ≥ {MIN_ROM_SIZE})",
            path.display(),
            bytes.len()
        )));
    }
    Ok(bytes)
}

impl MachineApp for Mtx {
    type Runtime = MtxRuntime;
    type Query = MtxSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-memotech-mtx";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --rom PATH      MTX ROM: 8 KB OS + paged ROMs (BASIC, ASSEM…); default
                    ~/.emu198x/roms/memotech-mtx/mtx.rom (or set EMU198X_MTX_ROM)
    --model KIND    mtx500 | mtx512 [default: mtx500]";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the MTX keyboard (cursor keys are real MTX keys)
    Shift / Ctrl    the MTX shift / control keys
    Gamepad         joystick (player 1)";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--rom" => self.rom = Some(args.path(flag)?),
            "--model" => {
                self.model = match args.value(flag)?.as_str() {
                    "mtx500" => Model::Mtx500,
                    "mtx512" => Model::Mtx512,
                    other => {
                        return Err(LaunchError::Usage(format!(
                            "--model expects mtx500|mtx512, got {other}"
                        )));
                    }
                };
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        FRAME_TICKS_PAL
    }

    fn query_provider(&self) -> MtxSessionQueryProvider {
        MtxSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<MtxRuntime, LaunchError> {
        let rom_path = resolve_rom(self.rom.as_deref(), ROM_ENV, ROM_RELATIVE)?;
        let rom = read_mtx_rom(&rom_path)?;
        MtxRuntime::new(self.model, rom)
            .map_err(|err| LaunchError::Run(format!("failed to construct runtime: {err}")))
    }

    /// MCP starts blank and takes the ROM from its conventional location when
    /// a well-sized image is there; a client can also hand it firmware later.
    fn build_mcp_runtime(&self) -> Result<MtxRuntime, LaunchError> {
        let mut runtime = MtxRuntime::blank(self.model);
        if let Ok(path) = resolve_rom(self.rom.as_deref(), ROM_ENV, ROM_RELATIVE)
            && let Ok(bytes) = std::fs::read(&path)
        {
            if rom_size_is_valid(bytes.len()) {
                runtime
                    .set_rom(bytes)
                    .map_err(|err| LaunchError::Run(format!("ROM invalid: {err}")))?;
                eprintln!("{} mcp: loaded ROM from {}", Self::BIN_NAME, path.display());
            } else {
                eprintln!(
                    "{} mcp: ROM at {} is {} bytes; expected the 8 KB OS \
                     plus 8 KB paged ROMs (a multiple of 8192) — starting blank",
                    Self::BIN_NAME,
                    path.display(),
                    bytes.len()
                );
            }
        }
        Ok(runtime)
    }

    fn report(&self, runtime: &MtxRuntime, report: &mut Map<String, Value>) {
        let frames_run = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("rom_loaded".to_owned(), runtime.machine().is_some().into());
        report.insert("frames_run".to_owned(), frames_run.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::HeadlessSession;
    use emu198x_shell::launch::{Mode, Parsed, parse};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn parse_cli_defaults() {
        let Parsed::Run { app, .. } = parse::<Mtx>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.rom.is_none());
        assert_eq!(app.model, Model::Mtx500);
    }

    #[test]
    fn parse_cli_accepts_rom_model_scale_video() {
        let parsed = parse::<Mtx>(&args(&[
            "--rom", "mtx.rom", "--model", "mtx512", "--scale", "4", "--video", "crt",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.rom, Some(PathBuf::from("mtx.rom")));
        assert_eq!(app.model, Model::Mtx512);
        assert_eq!(common.scale, Some(4));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn a_bad_model_is_a_usage_error() {
        let err = parse::<Mtx>(&args(&["--model", "mtx1000"])).expect_err("rejects");
        assert_eq!(
            err,
            LaunchError::Usage("--model expects mtx500|mtx512, got mtx1000".to_owned())
        );
    }

    #[test]
    fn native_budget_runs_exact_requested_frame_count() {
        let runtime = MtxRuntime::new(Model::Mtx500, vec![0; 16 * 1024]).expect("valid test ROM");
        let mut session = HeadlessSession::new(runtime, FRAME_TICKS_PAL);

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

    /// The budget must be the *largest* value that still never overshoots:
    /// equal to the shortest real frame. Any larger and `run_frames(n)`
    /// crosses a boundary and emits n+1 frames; any smaller and the per-frame
    /// shortfall eventually swallows a whole frame on a long run.
    #[test]
    fn native_budget_equals_the_shortest_real_frame() {
        let runtime = MtxRuntime::new(Model::Mtx500, vec![0; 16 * 1024]).expect("valid test ROM");
        let mut session = HeadlessSession::new(runtime, FRAME_TICKS_PAL);

        let mut shortest = u64::MAX;
        for expected in 1..=4 {
            let before = session.time().0;
            session.run_frames(1).expect("one frame");
            // Each call must advance exactly one frame, so the elapsed tick
            // count is that frame's true length.
            assert_eq!(
                session.machine().machine().expect("machine").frame_count(),
                expected,
                "budget {FRAME_TICKS_PAL} crossed a frame boundary",
            );
            shortest = shortest.min(session.time().0 - before);
        }

        assert_eq!(
            FRAME_TICKS_PAL, shortest,
            "budget should equal the shortest real frame",
        );
    }
}
