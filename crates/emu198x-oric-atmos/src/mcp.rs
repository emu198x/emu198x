//! MCP server mode — `--mcp` / `--mcp-stdio`.

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::{register_ay_watch_tools, register_base_tools, register_keyboard_tools},
};
use runtime_oric_atmos::{Model, OricRuntime, OricSessionQueryProvider};

// Oric: 6502 @ 1 MHz, 50 Hz PAL → ~20,000 cycles/frame.
const FRAME_TICKS: u64 = 20_000;

/// Runs MCP mode. Starts blank — ROM arrives via firmware load.
///
/// # Errors
///
/// Returns an error string if the JSON-RPC stdio loop hits an I/O failure.
pub fn run(args: &[String]) -> Result<(), String> {
    let machine = OricRuntime::blank(Model::Atmos);
    let mut session =
        HeadlessSession::new_with_query_provider(machine, FRAME_TICKS, OricSessionQueryProvider);

    // Media named on the command line is loaded here so `--rom`
    // means the same thing in MCP mode as in the other two (#1180).
    emu198x_shell::startup_media::load_into(&mut session, args)?;
    let mut server = Server::new(ServerInfo::new(
        "emu198x-oric-atmos",
        env!("CARGO_PKG_VERSION"),
    ));
    register_base_tools(server.registry_mut());
    // The machine has a keyboard, so the shared press_key / type_string apply.
    register_keyboard_tools(server.registry_mut());
    // The Oric carries an AY-3-8912 (PSG), so the shared AY-watch verbs apply.
    register_ay_watch_tools(server.registry_mut());
    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}
