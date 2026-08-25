//! MCP server mode — `--mcp` / `--mcp-stdio`.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::{register_base_tools, register_keyboard_tools},
};
use machine_jupiter_ace::TSTATES_PER_FRAME;
use runtime_jupiter_ace::{JupiterAceRuntime, JupiterAceSessionQueryProvider, Model};

// One exact display frame. A rounded 65,000-tick budget is longer than the
// 64,896-tick machine frame, so `run_frames(1)` would execute two frames.
const FRAME_TICKS: u64 = TSTATES_PER_FRAME as u64;

/// Runs MCP mode. Starts blank — ROM arrives via firmware load.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run(args: &[String]) -> Result<(), String> {
    let machine = JupiterAceRuntime::blank(Model::Ace3k);
    let mut session = HeadlessSession::new_with_query_provider(
        machine,
        FRAME_TICKS,
        JupiterAceSessionQueryProvider,
    );

    // Media named on the command line is loaded here so `--rom`
    // means the same thing in MCP mode as in the other two (#1180).
    emu198x_shell::startup_media::load_into(&mut session, args)?;
    let mut server = Server::new(ServerInfo::new(
        "emu198x-jupiter-ace",
        env!("CARGO_PKG_VERSION"),
    ));
    register_base_tools(server.registry_mut());
    // The machine has a keyboard, so the shared press_key / type_string apply.
    register_keyboard_tools(server.registry_mut());
    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

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
