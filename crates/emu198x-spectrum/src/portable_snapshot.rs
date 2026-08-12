//! Portable-snapshot helpers shared by every entry point.
//!
//! The GUI, the script runner, and the MCP server all need to load
//! `.sna` / `.z80` files (optionally wrapped in a `.zip`) into the live
//! machine. The classification and parsing are identical; only the
//! "where do we apply the result" step differs per entry point.
//!
//! This module owns the classifier and the parser. Callers handle their
//! own session-side prerequisites — `is_recording` guards, the
//! per-runtime apply call through `SpectrumLiveAccess::apply_snapshot`,
//! and error-type mapping — so this stays free of any session type.

use std::path::Path;

use common_sinclair_zx_spectrum::snapshot::Snapshot;
use emu198x_shell::{MediaKind, read_media_asset};

use crate::AppError;

/// True for paths the shared portable-snapshot reader handles:
/// `.sna`, `.z80`, or `.zip` (which `read_media_asset` unpacks to a
/// single inner `.sna` / `.z80`). Anything else falls through to the
/// shell crate's `restore_snapshot`, which decodes the runtime's own
/// postcard save state.
#[must_use]
pub fn is_portable_snapshot_path(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "sna" | "z80" | "szx" | "zip"
    )
}

/// Reads + parses a portable snapshot from disk. Returns the parsed
/// `Snapshot` ready to feed through `SpectrumLiveAccess::apply_snapshot`.
///
/// # Errors
///
/// Returns [`AppError::Io`] when the path cannot be read or the inner
/// file's extension is unrecognised; the underlying parser surface
/// (`format_sinclair_zx_spectrum_sna` / `format_sinclair_zx_spectrum_z80`)
/// returns parse errors via the same path.
pub fn parse_portable_snapshot_at(path: &Path) -> Result<Snapshot, AppError> {
    let loaded = read_media_asset(path, MediaKind::Snapshot)?;
    let inner_name = loaded
        .archive_member
        .as_deref()
        .unwrap_or_else(|| path.file_name().and_then(|n| n.to_str()).unwrap_or(""));
    let inner_lower = inner_name.to_ascii_lowercase();
    if inner_lower.ends_with(".sna") {
        format_sinclair_zx_spectrum_sna::parse_sna(&loaded.bytes)
            .map_err(|err| AppError::Io(std::io::Error::other(err)))
    } else if inner_lower.ends_with(".z80") {
        format_sinclair_zx_spectrum_z80::parse_z80(&loaded.bytes)
            .map_err(|err| AppError::Io(std::io::Error::other(err)))
    } else {
        Err(AppError::Io(std::io::Error::other(format!(
            "unrecognised snapshot at {} (expected .sna or .z80, got {inner_name})",
            path.display()
        ))))
    }
}
