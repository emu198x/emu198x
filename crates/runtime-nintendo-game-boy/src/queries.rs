//! Family-owned query surface for the Game Boy runtime.
//!
//! Splits the `SessionQueryProvider` impl out of `runtime.rs` so the
//! Game Boy query catalogue and its handlers have one home. The
//! provider itself is stateless (`GameBoySessionQueryProvider`); all
//! the lookup logic lives here.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use serde_json::json;

use crate::runtime::GameBoyRuntime;

/// Every path the Game Boy runtime answers via `query()`.
pub(crate) const GAME_BOY_QUERY_PATHS: &[&str] = &["cartridge.loaded", "cpu.pc"];

/// Game Boy-family query provider layered above the shared shell surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameBoySessionQueryProvider;

impl SessionQueryProvider<GameBoyRuntime> for GameBoySessionQueryProvider {
    fn query_paths(&self, _machine: &GameBoyRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = GAME_BOY_QUERY_PATHS
            .iter()
            .copied()
            .filter(|path| prefix.is_none_or(|prefix| path.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(
        &self,
        machine: &GameBoyRuntime,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "cartridge.loaded" => json!(machine.machine().is_some()),
            "cpu.pc" => json!(
                machine
                    .machine()
                    .ok_or_else(|| QueryError::UnavailablePath {
                        path: path.to_owned(),
                        reason: "no cartridge is loaded",
                    })?
                    .cpu_pc()
            ),
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
    use super::{GAME_BOY_QUERY_PATHS, GameBoySessionQueryProvider};
    use crate::{Model, runtime::GameBoyRuntime};
    use emu198x_shell::{QueryError, SessionQueryProvider};

    /// Catalogue invariant: every advertised path is unique. Doubles
    /// would silently clobber each other in a sorted query_paths
    /// listing.
    #[test]
    fn advertised_query_paths_are_unique() {
        let mut sorted: Vec<&&str> = GAME_BOY_QUERY_PATHS.iter().collect();
        sorted.sort();
        let mut deduped = sorted.clone();
        deduped.dedup();
        assert_eq!(sorted.len(), deduped.len(), "duplicate query paths");
    }

    /// Unknown query paths take the `_ => return Ok(None)` arm so the
    /// session shell can advertise its own paths without colliding
    /// with the family catalogue.
    #[test]
    fn unknown_query_path_returns_none() {
        let runtime = GameBoyRuntime::blank(Model::Dmg);
        let provider = GameBoySessionQueryProvider;
        assert!(
            provider
                .query(&runtime, "does.not.exist")
                .expect("unknown path should not error")
                .is_none()
        );
    }

    /// `gameboy.cpu.pc` requires a loaded cartridge; without one the
    /// provider must return `UnavailablePath` rather than panicking
    /// inside `cpu_pc()`.
    #[test]
    fn cpu_pc_without_cartridge_reports_unavailable() {
        let runtime = GameBoyRuntime::blank(Model::Dmg);
        let provider = GameBoySessionQueryProvider;
        let err = provider
            .query(&runtime, "cpu.pc")
            .expect_err("cpu.pc without a cartridge should error");
        match err {
            QueryError::UnavailablePath { path, .. } => {
                assert_eq!(path, "cpu.pc");
            }
            other => panic!("expected UnavailablePath, got {other:?}"),
        }
    }

    /// `query_paths` with no prefix returns the full catalogue sorted.
    #[test]
    fn query_paths_without_prefix_returns_full_catalogue() {
        let runtime = GameBoyRuntime::blank(Model::Dmg);
        let provider = GameBoySessionQueryProvider;
        let paths = provider.query_paths(&runtime, None);
        assert_eq!(paths.len(), GAME_BOY_QUERY_PATHS.len());
    }

    /// `query_paths` with a non-matching prefix returns an empty list.
    #[test]
    fn query_paths_with_no_match_returns_empty() {
        let runtime = GameBoyRuntime::blank(Model::Dmg);
        let provider = GameBoySessionQueryProvider;
        let paths = provider.query_paths(&runtime, Some("nope."));
        assert!(paths.is_empty());
    }
}
