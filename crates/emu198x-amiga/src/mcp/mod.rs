//! The Amiga MCP tool set (Stage Q).
//!
//! The server itself is the shared launcher's: `app.rs` registers these
//! tools on it through `MachineApp::register_mcp_tools`, and the launcher
//! boots one machine, loads any media named on the command line, and
//! drives the JSON-RPC stdio loop until stdin closes.
//!
//! The Amiga MCP exists primarily as a *debugging surface* for the
//! KS-internals investigation (Stage P onward). It exposes raw chip
//! state — CPU registers, copper-list address, BPLCON0, CIA timers —
//! rather than the higher-level `ScriptStep` shape the Spectrum uses,
//! because the questions we're asking ("what's A5 right now?", "what
//! does graphics.library Text() actually do?") need that level of
//! access.
//!
//! Since Stage AE-b/c/d/e, every chip-level tool drives the active
//! chipset variant through the [`AmigaLiveAccess`] trait — `--model`
//! picks OCS / ECS / AGA at boot time and the same tool set works
//! against any of them. AGA-only tooling (`query_aga`) gracefully
//! routes to the A1200 downcast.
//!
//! Default `--model` is `a500` (Kickstart 1.3) — the canonical Amiga
//! that vAmiga / FS-UAE / WinUAE also default to. Pass `--model a1200`
//! for the AGA chipset.
//!
//! ROM resolution is the same as the windowed and script modes':
//!
//!   1. `--kickstart PATH` explicit
//!   2. `--rom-dir DIR` directory
//!   3. `EMU198X_AMIGA_ROM_DIR` env var
//!   4. `~/.emu198x/roms/commodore-amiga/` or `~/.emu198x/roms/amiga/`
//!
//! Per-model candidate ROM names live in
//! [`crate::model::rom_candidates_for_model`].
//!
//! [`AmigaLiveAccess`]: runtime_commodore_amiga::AmigaLiveAccess

mod lvo;
pub(crate) mod tools;
