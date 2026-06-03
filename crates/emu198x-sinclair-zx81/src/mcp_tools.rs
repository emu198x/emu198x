//! Sinclair ZX81-specific MCP tools.
//!
//! The CPU / memory / disasm / stepping / I/O-trace surface is the shared
//! [`emu198x_shell::mcp_tools::register_debug_tools`] set; no
//! Sinclair ZX81-specific tools are needed yet.

use emu198x_shell::HeadlessSession;
use emu198x_shell::mcp::ToolRegistry;
use runtime_sinclair_zx81::{Zx81Runtime, Zx81SessionQueryProvider};

type Zx81Session = HeadlessSession<Zx81Runtime, Zx81SessionQueryProvider>;

/// Register the Sinclair ZX81 MCP tool surface: the shared debug tools.
pub fn register_zx81_tools(registry: &mut ToolRegistry<Zx81Session>) {
    emu198x_shell::mcp_tools::register_debug_tools(registry);
}
