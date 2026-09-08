//! The Memotech MTX as a [`MachineApp`]: its flags, runtime, and report fields.

#[cfg(test)]
use std::path::PathBuf;

use emu198x_shell::launch::{Args, LaunchError, MachineApp};
use emu198x_shell::{FirmwareOverrides, build_variant, build_variant_or_blank};
use runtime_memotech_mtx::{Model, MtxRuntime, MtxSessionQueryProvider};
use serde_json::{Map, Value};

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
#[cfg(test)]
pub const FRAME_TICKS_PAL: u64 = Model::Mtx500.frame_ticks();

/// The machine configuration the flags build up.
#[derive(Debug, PartialEq, Eq)]
pub struct Mtx {
    pub firmware: FirmwareOverrides,
    pub model: Model,
}

impl Default for Mtx {
    fn default() -> Self {
        Self {
            firmware: FirmwareOverrides::none(),
            model: Model::Mtx500,
        }
    }
}

impl MachineApp for Mtx {
    type Runtime = MtxRuntime;
    type Query = MtxSessionQueryProvider;

    const BIN_NAME: &'static str = "emu198x-memotech-mtx";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const MACHINE_OPTIONS: &'static str =
        "    --rom PATH|ID=PATH MTX ROM: 8 KB OS + paged ROMs (BASIC, ASSEM…); default
                    ~/.emu198x/roms/memotech-mtx/mtx.rom (or set EMU198X_MTX_ROM)
    --rom-dir DIR   firmware directory (or set EMU198X_MTX_ROM_DIR)
                    firmware ID: memotech-mtx-rom
    --model KIND    mtx500 | mtx512 [default: mtx500]";
    const CONTROLS: &'static str = "    Esc             quit
    F12             hard reset
    A-Z 0-9 etc.    the MTX keyboard (cursor keys are real MTX keys)
    Shift / Ctrl    the MTX shift / control keys
    Gamepad         joystick (player 1)";

    fn parse_flag(&mut self, flag: &str, args: &mut Args) -> Result<bool, LaunchError> {
        match flag {
            "--rom" => self
                .firmware
                .add_spec(
                    &args.value(flag)?,
                    self.model.variant_id(),
                    &self.model.firmware_sources(),
                )
                .map_err(|err| LaunchError::Usage(err.to_string()))?,
            "--rom-dir" => self.firmware.dir = Some(args.path(flag)?),
            "--model" => {
                let id = args.value(flag)?;
                self.model = Model::from_variant_id(&id).ok_or_else(|| {
                    LaunchError::Usage(format!(
                        "unknown memotech-mtx model `{id}`; expected {}",
                        Model::VARIANT_IDS.join(", ")
                    ))
                })?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn frame_ticks(&self) -> u64 {
        self.model.frame_ticks()
    }

    fn query_provider(&self) -> MtxSessionQueryProvider {
        MtxSessionQueryProvider
    }

    fn build_runtime(&self) -> Result<MtxRuntime, LaunchError> {
        build_variant::<MtxRuntime>(self.model, &self.firmware)
            .map_err(|err| LaunchError::Run(err.to_string()))
    }

    /// Conventional firmware is loaded when available. Missing conventional
    /// firmware permits blank startup; invalid images and explicit paths fail.
    fn build_mcp_runtime(&self) -> Result<MtxRuntime, LaunchError> {
        build_variant_or_blank(self.model, &self.firmware, MtxRuntime::blank)
            .map_err(|err| LaunchError::Run(err.to_string()))
    }

    fn mcp_startup_media(
        &self,
        _slots: &[emu198x_shell::MediaSlot],
        _raw_args: &[String],
    ) -> Result<Vec<(String, emu198x_shell::MediaKind, Vec<u8>)>, LaunchError> {
        self.startup_media()
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
        assert_eq!(app.firmware, FirmwareOverrides::none());
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
        assert_eq!(
            app.firmware
                .by_id
                .get(runtime_memotech_mtx::ROM_FIRMWARE_ID),
            Some(&PathBuf::from("mtx.rom"))
        );
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
            LaunchError::Usage(
                "unknown memotech-mtx model `mtx1000`; expected mtx500, mtx512".to_owned()
            )
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
