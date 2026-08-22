//! MCP server mode — `--mcp` / `--mcp-stdio`.

use std::env;
use std::fs;
use std::path::PathBuf;

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::{register_ay_watch_tools, register_base_tools, register_keyboard_tools},
};
use runtime_tatung_einstein::{EinsteinRuntime, EinsteinSessionQueryProvider, Model};

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
const FRAME_TICKS_PAL: u64 = 79_746;

/// Runs MCP mode.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run() -> Result<(), String> {
    let mut machine = EinsteinRuntime::blank(Model::Einstein);
    if let Some(path) = rom_path()
        && let Ok(bytes) = fs::read(&path)
    {
        if bytes.len() == 8 * 1024 {
            machine
                .set_rom(bytes)
                .map_err(|err| format!("ROM invalid: {err}"))?;
            eprintln!(
                "emu198x-tatung-einstein mcp: loaded MOS from {}",
                path.display()
            );
        } else {
            eprintln!(
                "emu198x-tatung-einstein mcp: MOS at {} is {} bytes; expected 8192 — starting blank",
                path.display(),
                bytes.len()
            );
        }
    }

    let mut session = HeadlessSession::new_with_query_provider(
        machine,
        FRAME_TICKS_PAL,
        EinsteinSessionQueryProvider,
    );
    let mut server = Server::new(ServerInfo::new(
        "emu198x-tatung-einstein",
        env!("CARGO_PKG_VERSION"),
    ));
    register_base_tools(server.registry_mut());
    // The machine has a keyboard, so the shared press_key / type_string apply.
    register_keyboard_tools(server.registry_mut());
    // The Einstein carries an AY-3-8910 (PSG), so the AY-watch verbs apply.
    register_ay_watch_tools(server.registry_mut());
    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}

fn rom_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_EINSTEIN_MOS")
        && !p.is_empty()
    {
        return Some(PathBuf::from(p));
    }
    let home = env::var("HOME").ok()?;
    let default = PathBuf::from(home).join(".emu198x/roms/tatung-einstein/mos.rom");
    if default.exists() {
        Some(default)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
