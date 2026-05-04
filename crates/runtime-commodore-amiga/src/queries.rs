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
use serde_json::json;

use crate::AmigaRuntime;
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
    "amiga.machine.frame_count",
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
            "amiga.machine.frame_count" => json!(machine.frame_count()),
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
        }
    }
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
