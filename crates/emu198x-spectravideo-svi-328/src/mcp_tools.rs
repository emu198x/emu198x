//! Spectravideo SVI-328-specific MCP tools.
//!
//! The CPU / memory / disasm / stepping / I/O-trace surface is the shared
//! [`emu198x_shell::mcp_tools::register_debug_tools`] set; no
//! Spectravideo SVI-328-specific tools are needed yet.

use emu198x_shell::HeadlessSession;
use emu198x_shell::mcp::ToolRegistry;
use runtime_spectravideo_svi_328::{Svi328Runtime, Svi328SessionQueryProvider};

type SviSession = HeadlessSession<Svi328Runtime, Svi328SessionQueryProvider>;

/// Register the Spectravideo SVI-328 MCP tool surface: the shared debug tools.
pub fn register_svi_tools(registry: &mut ToolRegistry<SviSession>) {
    emu198x_shell::mcp_tools::register_debug_tools(registry);
}
