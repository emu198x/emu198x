//! BBC Micro-specific MCP tools.
//!
//! The CPU / memory / stepping surface is the shared
//! [`emu198x_shell::mcp_tools::register_debug_tools`] set (6502 `disasm`
//! is pending the Asm198x crate; `io_trace` is unsupported on this
//! memory-mapped CPU). No BBC Micro-specific tools are needed yet.

use emu198x_shell::HeadlessSession;
use emu198x_shell::mcp::ToolRegistry;
use runtime_acorn_bbc_micro::{BbcMicroRuntime, BbcMicroSessionQueryProvider};

type BbcSession = HeadlessSession<BbcMicroRuntime, BbcMicroSessionQueryProvider>;

/// Register the BBC Micro MCP tool surface: the shared debug tools.
pub fn register_bbc_tools(registry: &mut ToolRegistry<BbcSession>) {
    emu198x_shell::mcp_tools::register_debug_tools(registry);
}
