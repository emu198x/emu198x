//! The Tatung Einstein as a [`MachineApp`]: its flags, runtime, and report fields.

use std::path::PathBuf;

use emu198x_shell::HeadlessSession;
use emu198x_shell::launch::{Args, LaunchError, MachineApp, read_rom_exact, resolve_rom};
use emu198x_shell::mcp::ToolRegistry;
use emu198x_shell::mcp_tools::{
    register_ay_watch_tools, register_base_tools, register_keyboard_tools,
};
use runtime_tatung_einstein::{EinsteinRuntime, EinsteinSessionQueryProvider, Model};
use serde_json::{Map, Value};

const MOS_ENV: &str = "EMU198X_EINSTEIN_MOS";
const MOS_RELATIVE: &str = "tatung-einstein/mos.rom";
const MOS_SIZE: usize = 8 * 1024;

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
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Einstein {
    pub mos: Option<PathBuf>,
}

impl MachineApp for Einstein {
    type Runtime = EinsteinRuntime;
    type Query = EinsteinSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-tatung-einstein";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str = "    --mos PATH      Einstein MOS ROM (8 KB); default
                    ~/.emu198x/roms/tatung-einstein/mos.rom
                    (or set EMU198X_EINSTEIN_MOS)";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the Einstein keyboard
    Shift / Ctrl    the Einstein SHIFT / CONTROL keys
    Gamepad         joystick (player 1)";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--mos" => self.mos = Some(args.path(flag)?),
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        FRAME_TICKS_PAL
    }

    fn query_provider(&self) -> EinsteinSessionQueryProvider {
        EinsteinSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<EinsteinRuntime, LaunchError> {
        let mos_path = resolve_rom(self.mos.as_deref(), MOS_ENV, MOS_RELATIVE).map_err(|_| {
            LaunchError::Run("no MOS ROM: pass --mos PATH or set EMU198X_EINSTEIN_MOS".to_owned())
        })?;
        let mos = read_rom_exact(&mos_path, "MOS ROM", MOS_SIZE)?;
        EinsteinRuntime::new(Model::Einstein, mos)
            .map_err(|err| LaunchError::Run(format!("failed to construct runtime: {err}")))
    }

    /// MCP starts blank and takes the MOS from its conventional location when
    /// an 8 KB image is there; a client can also hand it firmware later.
    fn build_mcp_runtime(&self) -> Result<EinsteinRuntime, LaunchError> {
        let mut runtime = EinsteinRuntime::blank(Model::Einstein);
        if let Ok(path) = resolve_rom(self.mos.as_deref(), MOS_ENV, MOS_RELATIVE)
            && let Ok(bytes) = std::fs::read(&path)
        {
            if bytes.len() == MOS_SIZE {
                runtime
                    .set_rom(bytes)
                    .map_err(|err| LaunchError::Run(format!("ROM invalid: {err}")))?;
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

    fn report(&self, runtime: &EinsteinRuntime, report: &mut Map<String, Value>) {
        let frames_run = runtime.machine().map_or(0, |m| m.frame_count());
        report.insert("mos_loaded".to_owned(), runtime.machine().is_some().into());
        report.insert("frames_run".to_owned(), frames_run.into());
    }

    fn register_mcp_tools(
        &self,
        registry: &mut ToolRegistry<HeadlessSession<EinsteinRuntime, EinsteinSessionQueryProvider>>,
    ) {
        register_base_tools(registry);
        // The machine has a keyboard, so the shared press_key / type_string apply.
        register_keyboard_tools(registry);
        // The Einstein carries an AY-3-8910 (PSG), so the AY-watch verbs apply.
        register_ay_watch_tools(registry);
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
        let Parsed::Run { app, .. } = parse::<Einstein>(&[]).expect("parses") else {
            panic!("expected a run");
        };
        assert!(app.mos.is_none());
    }

    #[test]
    fn parse_cli_accepts_mos_scale_video() {
        let parsed = parse::<Einstein>(&args(&[
            "--mos", "mos.rom", "--scale", "4", "--video", "crt",
        ]))
        .expect("parses");
        let Parsed::Run { app, common, mode } = parsed else {
            panic!("expected a run");
        };
        assert_eq!(app.mos, Some(PathBuf::from("mos.rom")));
        assert_eq!(common.scale, Some(4));
        assert_eq!(common.video.as_deref(), Some("crt"));
        assert_eq!(mode, Mode::Ui);
    }

    #[test]
    fn native_budget_runs_exact_requested_frame_count() {
        let runtime =
            EinsteinRuntime::new(Model::Einstein, vec![0; 8 * 1024]).expect("valid test ROM");
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
        let runtime =
            EinsteinRuntime::new(Model::Einstein, vec![0; 8 * 1024]).expect("valid test ROM");
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
