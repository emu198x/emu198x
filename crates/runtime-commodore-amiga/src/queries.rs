//! Family-owned query surface for the Amiga runtime.
//!
//! Splits the `SessionQueryProvider` impl out of `runtime.rs` so the
//! query path catalogue, the boot-status heuristic, and the dispatch
//! table all live alongside each other. The provider itself is
//! stateless (`AmigaSessionQueryProvider`); all the lookup logic lives
//! here.
//!
//! The provider is generic over `M: AmigaMachine` so a single type
//! covers every present and future variant. Variant-specific paths
//! (anything outside the runtime-owned `boot.*` and
//! `amiga.machine.*` namespaces) are pushed down to the machine via
//! `M::resolve_variant_query`.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use serde_json::{Value, json};

use crate::AmigaRuntime;
use crate::live_access::AmigaLiveAccess;
use crate::variants::AmigaMachine;

/// Runtime-owned query paths shared by every Amiga variant. Variant-
/// specific paths come from `M::variant_query_paths()` and are joined
/// in by `query_paths`.
pub(crate) const SHARED_QUERY_PATHS: &[&str] = &[
    // Boot-status heuristic. `HeadlessSession::wait_for_boot` keys
    // off `boot.detected` so scripts can sleep-until-ready.
    "boot.detected",
    "boot.reason",
    "boot.row",
    "machine.frame_count",
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
pub(crate) fn boot_status<M: AmigaMachine>(runtime: &AmigaRuntime<M>) -> AmigaBootStatus {
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

impl<M: AmigaMachine> SessionQueryProvider<AmigaRuntime<M>> for AmigaSessionQueryProvider {
    fn query_paths(&self, _machine: &AmigaRuntime<M>, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = SHARED_QUERY_PATHS
            .iter()
            .chain(M::variant_query_paths().iter())
            .copied()
            .filter(|path| prefix.is_none_or(|prefix| path.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths.dedup();
        paths
    }

    fn query(
        &self,
        machine: &AmigaRuntime<M>,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        // Runtime-owned paths come first.
        let value = match path {
            "boot.detected" => json!(boot_status(machine).detected),
            "boot.reason" => json!(boot_status(machine).reason),
            "boot.row" => json!(boot_status(machine).row),
            "machine.frame_count" => json!(machine.frame_count()),
            _ => {
                // Push everything else down to the variant.
                return match machine.machine().resolve_variant_query(path)? {
                    Some(value) => Ok(Some(QueryResult {
                        path: path.to_owned(),
                        value,
                    })),
                    None => Ok(None),
                };
            }
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

/// Same provider, but dispatching over the runtime-time
/// `AmigaRuntimeKind` enum so verifier binaries that store
/// `AmigaRuntimeKind` (rather than a concrete `AmigaOcsRuntime` /
/// `AmigaEcsRuntime`) can use this provider directly. The OCS and
/// ECS impl blocks share the same query catalogue today, so the
/// dispatch is trivial.
impl SessionQueryProvider<crate::variants::AmigaRuntimeKind> for AmigaSessionQueryProvider {
    fn query_paths(
        &self,
        machine: &crate::variants::AmigaRuntimeKind,
        prefix: Option<&str>,
    ) -> Vec<String> {
        match machine {
            crate::variants::AmigaRuntimeKind::Ocs(rt) => self.query_paths(rt, prefix),
            crate::variants::AmigaRuntimeKind::Ecs(rt) => self.query_paths(rt, prefix),
            crate::variants::AmigaRuntimeKind::Aga(rt) => self.query_paths(rt, prefix),
        }
    }

    fn query(
        &self,
        machine: &crate::variants::AmigaRuntimeKind,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        match machine {
            crate::variants::AmigaRuntimeKind::Ocs(rt) => self.query(rt, path),
            crate::variants::AmigaRuntimeKind::Ecs(rt) => self.query(rt, path),
            crate::variants::AmigaRuntimeKind::Aga(rt) => self.query(rt, path),
        }
    }
}

// ===================================================================
// Folded chip-snapshot query paths (#456)
//
// The bespoke `query_agnus` / `query_paula` / `query_cia` /
// `query_blitter` / `query_chipset` / `query_disk` / `query_aga` MCP
// tools became grouped objects (`agnus`, …) plus per-field leaves on the
// generic `query` surface. The builders run over `&dyn AmigaLiveAccess`
// — the same trait the old tools used — so one set of helpers serves
// every variant's `resolve_variant_query`. Values are raw numbers,
// matching the existing Amiga leaves and the rest of the fleet.
// ===================================================================

/// Does `path` address chip `chip` — the bare group name or a dotted
/// leaf beneath it (`agnus`, `agnus.vpos`)?
pub(crate) fn is_chip(path: &str, chip: &str) -> bool {
    path == chip
        || path
            .strip_prefix(chip)
            .is_some_and(|rest| rest.starts_with('.'))
}

/// Resolve a chip path against a built snapshot: the bare group name
/// returns the whole object, a `chip.field` leaf returns that field, and
/// an unknown sub-field returns `None` (an unknown path, not a null).
/// Returns the bare `Value` the Amiga `resolve_variant_query` contract
/// expects (the provider wraps it into a `QueryResult`).
pub(crate) fn chip_field(path: &str, chip: &str, snapshot: Value) -> Option<Value> {
    if path == chip {
        return Some(snapshot);
    }
    let field = path.strip_prefix(chip)?.strip_prefix('.')?;
    snapshot.get(field).cloned()
}

/// Decode the Paula INTENA/INTREQ bit layout into a readable map.
/// Bit 14 = master enable; bits 13..0 are individual interrupt sources.
fn decode_int_bits(val: u16) -> Value {
    const NAMES: [&str; 15] = [
        "TBE", "DSKBLK", "SOFT", "PORTS", "COPER", "VERTB", "BLIT", "AUD0", "AUD1", "AUD2", "AUD3",
        "RBF", "DSKSYN", "EXTER", "INTEN",
    ];
    let mut out = serde_json::Map::new();
    for (bit, name) in NAMES.iter().enumerate() {
        if val & (1 << bit) != 0 {
            out.insert((*name).to_string(), Value::Bool(true));
        }
    }
    Value::Object(out)
}

/// BPLCON0 / DMACON / ADKCON / COLOR00 / copper pointers / overlay.
pub(crate) fn chipset_snapshot(m: &dyn AmigaLiveAccess) -> Value {
    json!({
        "bplcon0": m.bplcon0(),
        "dmacon": m.dmacon(),
        "adkcon": m.adkcon(),
        "color00": m.color(0),
        "cop1lc": m.copper_cop1lc(),
        "cop2lc": m.copper_cop2lc(),
        "copper_pc": m.copper_pc(),
        "overlay": m.overlay(),
    })
}

/// Paula interrupt state: INTENA / INTREQ raw plus the master-enable
/// flag and the decoded source bitmaps.
pub(crate) fn paula_snapshot(m: &dyn AmigaLiveAccess) -> Value {
    let intena = m.intena();
    let intreq = m.intreq();
    json!({
        "intena": intena,
        "intreq": intreq,
        "master_enable": (intena & 0x4000) != 0,
        "intena_bits": decode_int_bits(intena),
        "intreq_bits": decode_int_bits(intreq),
    })
}

/// One CIA-8520's timer + control + TOD + port register file.
fn cia_fields(c: &machine_commodore_amiga_ocs::Cia) -> Value {
    json!({
        "cra": c.cra(),
        "crb": c.crb(),
        "timer_a": c.timer_a(),
        "timer_b": c.timer_b(),
        "timer_a_running": c.timer_a_running(),
        "timer_b_running": c.timer_b_running(),
        "icr_status": c.icr_status(),
        "icr_mask": c.icr_mask(),
        "irq_active": c.irq_active(),
        "ddr_a": c.ddr_a(),
        "ddr_b": c.ddr_b(),
        "port_a_output": c.port_a_output(),
        "port_b_output": c.port_b_output(),
        "tod_counter": c.tod_counter(),
        "tod_alarm": c.tod_alarm(),
        "tod_halted": c.tod_halted(),
    })
}

/// Both CIAs (`cia_a` = U7 / keyboard / floppy control, `cia_b` = U8 /
/// serial / disk step).
pub(crate) fn cia_snapshot(m: &dyn AmigaLiveAccess) -> Value {
    json!({
        "cia_a": cia_fields(m.cia_a()),
        "cia_b": cia_fields(m.cia_b()),
    })
}

/// Agnus: beam position, DMA pointers, blitter pointers, the display
/// window / data-fetch registers, modulos, and the fetch-width / plane
/// decode. `dmacon` / `bplcon0` mirror the chipset registers Agnus owns.
pub(crate) fn agnus_snapshot(m: &dyn AmigaLiveAccess) -> Value {
    let a = m.agnus();
    json!({
        "vpos": a.vpos,
        "hpos": a.hpos,
        "dmacon": m.dmacon(),
        "bplcon0": m.bplcon0(),
        "blitter_busy": a.blitter_busy,
        "blitter_busy_visible": a.blitter_busy_visible(),
        "blitter_exec_pending": a.blitter_exec_pending,
        "blitter_startup_ccks_remaining": a.blitter_startup_ccks_remaining(),
        "blitter_ccks_remaining": a.blitter_ccks_remaining,
        "bpl_pt": (0..8).map(|i| a.bpl_pt[i]).collect::<Vec<_>>(),
        "blt_apt": a.blt_apt,
        "blt_bpt": a.blt_bpt,
        "blt_cpt": a.blt_cpt,
        "blt_dpt": a.blt_dpt,
        "fmode": a.fmode,
        "bpl_fetch_width": a.bpl_fetch_width(),
        "spr_fetch_width": a.spr_fetch_width(),
        "diwstrt": a.diwstrt,
        "diwstop": a.diwstop,
        "ddfstrt": a.ddfstrt,
        "ddfstop": a.ddfstop,
        "bpl1mod": a.bpl1mod,
        "bpl2mod": a.bpl2mod,
        "num_bitplanes": a.num_bitplanes(),
    })
}

/// Blitter sub-view of Agnus: busy / pending state and the A-D channel
/// pointers.
pub(crate) fn blitter_snapshot(m: &dyn AmigaLiveAccess) -> Value {
    let a = m.agnus();
    json!({
        "busy": a.blitter_busy,
        "busy_visible": a.blitter_busy_visible(),
        "exec_pending": a.blitter_exec_pending,
        "startup_ccks_remaining": a.blitter_startup_ccks_remaining(),
        "ccks_remaining": a.blitter_ccks_remaining,
        "apt": a.blt_apt,
        "bpt": a.blt_bpt,
        "cpt": a.blt_cpt,
        "dpt": a.blt_dpt,
    })
}

/// DF0 drive state. Field names preserve the pre-fold `disk.*` leaves
/// (`inserted`, `change_pending`, `motor_spinning` = the active-low
/// READY line) and add `selected` plus the raw four-line `status`.
pub(crate) fn disk_snapshot(m: &dyn AmigaLiveAccess) -> Value {
    let drive = m.drive();
    let status = drive.status();
    json!({
        "inserted": drive.has_disk(),
        "change_pending": status.disk_change,
        "cylinder": drive.cylinder(),
        "head": drive.head(),
        "motor_on": drive.motor_on(),
        "motor_spinning": status.ready,
        "step_events": drive.step_event_counter(),
        "selected": drive.selected(),
        "status": {
            "disk_change_low": status.disk_change,
            "write_protect_low": status.write_protect,
            "track0_low": status.track0,
            "ready_low": status.ready,
        },
    })
}

/// Dispatch the chipset chip groups shared by every variant (`agnus`,
/// `paula`, `cia`, `blitter`, `chipset`, `disk`). Returns `Some(value)`
/// for an owned group or leaf, and `None` both for a non-chip path and
/// for an unknown sub-field — the caller's own match then handles the
/// former and reports the latter as an unknown path. The AGA-only `aga`
/// group is routed separately by the AGA variant.
pub(crate) fn resolve_chip_query(m: &dyn AmigaLiveAccess, path: &str) -> Option<Value> {
    if is_chip(path, "agnus") {
        return chip_field(path, "agnus", agnus_snapshot(m));
    }
    if is_chip(path, "paula") {
        return chip_field(path, "paula", paula_snapshot(m));
    }
    if is_chip(path, "cia") {
        return chip_field(path, "cia", cia_snapshot(m));
    }
    if is_chip(path, "blitter") {
        return chip_field(path, "blitter", blitter_snapshot(m));
    }
    if is_chip(path, "chipset") {
        return chip_field(path, "chipset", chipset_snapshot(m));
    }
    if is_chip(path, "disk") {
        return chip_field(path, "disk", disk_snapshot(m));
    }
    None
}

/// AGA Lisa register + palette snapshot. AGA-only; the caller routes
/// this path only on the A1200 variant, where `aga_lisa()` is `Some`.
pub(crate) fn aga_snapshot(m: &dyn AmigaLiveAccess) -> Option<Value> {
    let aga = m.aga_lisa()?;
    let bplcon3 = aga.bplcon3;
    let mut bank_nonzero: [u32; 8] = [0; 8];
    for (i, &c) in aga.palette_24.iter().enumerate() {
        if c != 0 {
            bank_nonzero[i / 32] += 1;
        }
    }
    let bank0: Vec<u32> = aga.palette_24[0..32].to_vec();
    let ocs_palette: Vec<u16> = (0..32).map(|i| m.color(i)).collect();
    Some(json!({
        "deniseid": aga.deniseid,
        "bplcon3": bplcon3,
        "bplcon3_bank": (bplcon3 >> 13) & 7,
        "bplcon3_loct": (bplcon3 & 0x0200) != 0,
        "bplcon4": aga.bplcon4,
        "spr_width": aga.spr_width,
        "ham_prev_rgb24": aga.ham_prev_rgb24,
        "palette_24_nonzero_per_bank": bank_nonzero,
        "palette_24_bank0": bank0,
        "ocs_palette_12bit": ocs_palette,
    }))
}

#[cfg(test)]
mod tests {
    use super::SHARED_QUERY_PATHS;

    /// Catalogue invariant: every advertised shared path is unique.
    /// Doubles would silently clobber each other in a sorted listing.
    /// The variant catalogues are checked separately in `variants.rs`
    /// (one test per variant impl).
    #[test]
    fn shared_query_paths_are_unique() {
        let mut sorted: Vec<&&str> = SHARED_QUERY_PATHS.iter().collect();
        sorted.sort();
        let mut deduped = sorted.clone();
        deduped.dedup();
        assert_eq!(sorted.len(), deduped.len(), "duplicate shared query paths");
    }
}
