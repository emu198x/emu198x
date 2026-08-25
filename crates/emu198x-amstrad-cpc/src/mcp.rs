//! MCP server mode — `--mcp` / `--mcp-stdio`.

use std::fs;

use emu198x_shell::{
    HeadlessSession,
    mcp::{Server, ServerInfo, serve_stdio},
    mcp_tools::{register_base_tools, register_keyboard_tools},
};
use runtime_amstrad_cpc::{AmstradCpcRuntime, AmstradCpcSessionQueryProvider, Model};

use crate::script::default_rom_path;

/// One PAL frame in T-states — the same budget the headless runner uses.
const FRAME_TICKS_PAL: u64 = 64 * 312 * 4;
const FIRMWARE_SIZE: usize = 32 * 1024;

/// Runs MCP mode.
///
/// # Errors
///
/// Returns an error string if the firmware is present but invalid, or if the
/// JSON-RPC stdio loop hits an I/O failure.
pub fn run(args: &[String]) -> Result<(), String> {
    let mut machine = AmstradCpcRuntime::blank(Model::Cpc464);
    if let Some(path) = default_rom_path().filter(|p| p.exists())
        && let Ok(bytes) = fs::read(&path)
    {
        if bytes.len() == FIRMWARE_SIZE {
            machine
                .set_firmware(bytes)
                .map_err(|err| format!("firmware invalid: {err}"))?;
            eprintln!(
                "emu198x-amstrad-cpc mcp: loaded firmware from {}",
                path.display()
            );
        } else {
            eprintln!(
                "emu198x-amstrad-cpc mcp: firmware at {} is {} bytes; expected {FIRMWARE_SIZE} — starting blank",
                path.display(),
                bytes.len()
            );
        }
    }

    let mut session = HeadlessSession::new_with_query_provider(
        machine,
        FRAME_TICKS_PAL,
        AmstradCpcSessionQueryProvider,
    );

    // Media named on the command line is loaded here so `--rom`
    // means the same thing in MCP mode as in the other two (#1180).
    emu198x_shell::startup_media::load_into(&mut session, args)?;
    let mut server = Server::new(ServerInfo::new(
        "emu198x-amstrad-cpc",
        env!("CARGO_PKG_VERSION"),
    ));
    register_base_tools(server.registry_mut());
    // The machine has a keyboard, so the shared press_key / type_string apply.
    register_keyboard_tools(server.registry_mut());
    // The AY-watch verbs are deliberately absent: the CPC's PSG is reachable
    // through the `psg.registers` query path, but the machine does not carry
    // the write-watch hook those tools drive, and registering a tool the
    // machine cannot serve advertises a capability that does not exist.
    serve_stdio(&mut server, &mut session).map_err(|err| err.to_string())
}
