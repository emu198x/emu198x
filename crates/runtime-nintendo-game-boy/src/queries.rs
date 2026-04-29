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
pub(crate) const GAME_BOY_QUERY_PATHS: &[&str] =
    &["gameboy.cartridge.loaded", "gameboy.cpu.pc"];

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
            "gameboy.cartridge.loaded" => json!(machine.machine().is_some()),
            "gameboy.cpu.pc" => json!(
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
    use super::GAME_BOY_QUERY_PATHS;

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
}
