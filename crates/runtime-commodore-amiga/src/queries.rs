//! Family-owned query surface for the Amiga runtime.
//!
//! Splits the `SessionQueryProvider` impl out of `runtime.rs` so the
//! query path catalogue, the boot-status heuristic, and the dispatch
//! table all live alongside each other. The provider itself is
//! stateless (`AmigaSessionQueryProvider`); all the lookup logic lives
//! here.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use serde_json::json;

use crate::runtime::AmigaRuntime;

/// Query paths the runtime publishes through the session query
/// provider. Kept deliberately short — shell diagnostics start here
/// and can grow as the verifier UI adds panels.
pub(crate) const AMIGA_QUERY_PATHS: &[&str] = &[
    // Boot-status heuristic. `HeadlessSession::wait_for_boot` keys
    // off `boot.detected` so scripts can sleep-until-ready.
    "boot.detected",
    "boot.reason",
    "boot.row",
    "amiga.a1000.boot_rom_visible",
    "amiga.a1000.wom_locked",
    "amiga.machine.frame_count",
    "amiga.memory.overlay",
    "amiga.cpu.pc",
    "amiga.cpu.sr",
    "amiga.cpu.ipl",
    "amiga.agnus.vpos",
    "amiga.agnus.hpos",
    "amiga.agnus.dmacon",
    "amiga.agnus.bplcon0",
    "amiga.paula.intena",
    "amiga.paula.intreq",
    "amiga.debug.dsk_write_count",
    "amiga.debug.last_dsk_write",
    "amiga.display.color00",
    "amiga.display.color01",
    "amiga.disk.inserted",
    "amiga.disk.change_pending",
    "amiga.disk.cylinder",
    "amiga.disk.head",
    "amiga.disk.motor_on",
    "amiga.disk.motor_spinning",
    "amiga.disk.step_events",
    "amiga.keyboard.state",
    "amiga.keyboard.queued",
];

/// Boot-status snapshot derived from the most recent frame. Matches
/// the archive's `AmigaBootStatus` heuristic: a mostly-coloured
/// framebuffer with visible pixels above row zero counts as boot-
/// detected, matching the Kickstart insert-disk screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AmigaBootStatus {
    pub detected: bool,
    pub reason: &'static str,
    pub row: Option<u32>,
}

/// Boot-status heuristic matching the archive's semantics:
///   - `display-active` once the framebuffer has mostly non-white
///     content and a non-zero first active row (Kickstart insert-disk
///     screen or beyond)
///   - `monochrome-framebuffer` if some pixels lit but below the
///     threshold
///   - `no-visible-output` before the copper has programmed the
///     palette at all
pub(crate) fn boot_status(runtime: &AmigaRuntime) -> AmigaBootStatus {
    if let Some(row) = runtime.first_active_row()
        && runtime.non_white_pixels() > 1_000
    {
        AmigaBootStatus {
            detected: true,
            reason: "display-active",
            row: Some(row),
        }
    } else if runtime.non_black_pixels() > 0 {
        AmigaBootStatus {
            detected: false,
            reason: "monochrome-framebuffer",
            row: runtime.first_active_row(),
        }
    } else {
        AmigaBootStatus {
            detected: false,
            reason: "no-visible-output",
            row: None,
        }
    }
}

/// Amiga-family query provider layered above the shared shell surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AmigaSessionQueryProvider;

impl SessionQueryProvider<AmigaRuntime> for AmigaSessionQueryProvider {
    fn query_paths(&self, _machine: &AmigaRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = AMIGA_QUERY_PATHS
            .iter()
            .copied()
            .filter(|path| prefix.is_none_or(|prefix| path.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(&self, machine: &AmigaRuntime, path: &str) -> Result<Option<QueryResult>, QueryError> {
        let amiga = machine.machine();
        let drive = amiga.drive();
        let drive_status = drive.status();
        let boot = boot_status(machine);
        let value = match path {
            "boot.detected" => json!(boot.detected),
            "boot.reason" => json!(boot.reason),
            "boot.row" => json!(boot.row),
            "amiga.a1000.boot_rom_visible" => json!(amiga.memory().a1000_boot_rom_visible()),
            "amiga.a1000.wom_locked" => json!(amiga.memory().a1000_wom_locked()),
            "amiga.machine.frame_count" => json!(machine.frame_count()),
            "amiga.memory.overlay" => json!(amiga.memory().overlay()),
            "amiga.cpu.pc" => json!(amiga.cpu().regs.pc),
            "amiga.cpu.sr" => json!(amiga.cpu().regs.sr),
            "amiga.cpu.ipl" => json!(amiga.cpu().ipl),
            "amiga.agnus.vpos" => json!(amiga.agnus().vpos),
            "amiga.agnus.hpos" => json!(amiga.agnus().hpos),
            "amiga.agnus.dmacon" => json!(amiga.dmacon()),
            "amiga.agnus.bplcon0" => json!(amiga.bplcon0()),
            "amiga.paula.intena" => json!(amiga.intena()),
            "amiga.paula.intreq" => json!(amiga.intreq()),
            "amiga.debug.dsk_write_count" => json!(amiga.debug_dsk_log.len()),
            "amiga.debug.last_dsk_write" => {
                json!(amiga.debug_dsk_log.last().map(|(cck, pc, reg, val)| {
                    json!({
                        "cck": cck,
                        "pc": pc,
                        "reg": reg,
                        "val": val,
                    })
                }))
            }
            "amiga.display.color00" => json!(amiga.color(0)),
            "amiga.display.color01" => json!(amiga.color(1)),
            "amiga.disk.inserted" => json!(drive.has_disk()),
            "amiga.disk.change_pending" => json!(drive_status.disk_change),
            "amiga.disk.cylinder" => json!(drive.cylinder()),
            "amiga.disk.head" => json!(drive.head()),
            "amiga.disk.motor_on" => json!(drive.motor_on()),
            "amiga.disk.motor_spinning" => json!(drive_status.ready),
            "amiga.disk.step_events" => json!(drive.step_event_counter()),
            "amiga.keyboard.state" => json!(amiga.keyboard().debug_state_name()),
            "amiga.keyboard.queued" => json!(amiga.keyboard().queued_key_count()),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::AMIGA_QUERY_PATHS;

    /// Catalogue invariant: every advertised path is unique. Doubles
    /// would silently clobber each other in a sorted query_paths
    /// listing.
    #[test]
    fn advertised_query_paths_are_unique() {
        let mut sorted: Vec<&&str> = AMIGA_QUERY_PATHS.iter().collect();
        sorted.sort();
        let mut deduped = sorted.clone();
        deduped.dedup();
        assert_eq!(sorted.len(), deduped.len(), "duplicate query paths");
    }
}
