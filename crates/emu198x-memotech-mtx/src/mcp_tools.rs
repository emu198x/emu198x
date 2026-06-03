//! Memotech MTX-specific MCP tools.
//!
//! The CPU / memory / disasm / stepping / I/O-trace surface is the shared
//! [`emu198x_shell::mcp_tools::register_debug_tools`] set; no
//! Memotech MTX-specific tools are needed yet.

use emu198x_shell::HeadlessSession;
use emu198x_shell::mcp::ToolRegistry;
use runtime_memotech_mtx::{MtxRuntime, MtxSessionQueryProvider};

type MtxSession = HeadlessSession<MtxRuntime, MtxSessionQueryProvider>;

/// Register the Memotech MTX MCP tool surface: the shared debug tools.
pub fn register_mtx_tools(registry: &mut ToolRegistry<MtxSession>) {
    emu198x_shell::mcp_tools::register_debug_tools(registry);
}
