//! Jupiter Ace-specific MCP tools.
//!
//! The CPU / memory / disasm / stepping / I/O-trace surface is the shared
//! [`emu198x_shell::mcp_tools::register_debug_tools`] set; no
//! Jupiter Ace-specific tools are needed yet.

use emu198x_shell::HeadlessSession;
use emu198x_shell::mcp::ToolRegistry;
use runtime_jupiter_ace::{JupiterAceRuntime, JupiterAceSessionQueryProvider};

type AceSession = HeadlessSession<JupiterAceRuntime, JupiterAceSessionQueryProvider>;

/// Register the Jupiter Ace MCP tool surface: the shared debug tools.
pub fn register_ace_tools(registry: &mut ToolRegistry<AceSession>) {
    emu198x_shell::mcp_tools::register_debug_tools(registry);
}
