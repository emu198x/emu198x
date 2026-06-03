//! Atari 5200-specific MCP tools.
//!
//! The CPU / memory / stepping surface is the shared
//! [`emu198x_shell::mcp_tools::register_debug_tools`] set (6502 `disasm`
//! pending the Asm198x crate; `io_trace` unsupported on this
//! memory-mapped CPU). No Atari 5200-specific tools are needed yet.

use emu198x_shell::HeadlessSession;
use emu198x_shell::mcp::ToolRegistry;
use runtime_atari_5200::{Atari5200Runtime, Atari5200SessionQueryProvider};

type A5200Session = HeadlessSession<Atari5200Runtime, Atari5200SessionQueryProvider>;

/// Register the Atari 5200 MCP tool surface: the shared debug tools.
pub fn register_a5200_tools(registry: &mut ToolRegistry<A5200Session>) {
    emu198x_shell::mcp_tools::register_debug_tools(registry);
}
