//! Family-owned query surface for the PET runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_commodore_pet::Pet;
use serde_json::json;

use crate::runtime::PetRuntime;

pub(crate) const PET_QUERY_PATHS: &[&str] = &[
    "cpu.pc",
    "firmware.loaded",
    "machine.frame_count",
    "machine.master_clock",
    "model.screen_chars",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PetSessionQueryProvider;

impl SessionQueryProvider<PetRuntime> for PetSessionQueryProvider {
    fn query_paths(&self, _machine: &PetRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = PET_QUERY_PATHS
            .iter()
            .copied()
            .filter(|p| prefix.is_none_or(|prefix| p.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(&self, machine: &PetRuntime, path: &str) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "firmware.loaded" => json!(machine.machine().is_some()),
            "model.screen_chars" => json!(machine.model().screen_chars()),
            "machine.frame_count" => json!(machine.machine().map_or(0, Pet::frame_count)),
            "machine.master_clock" => json!(machine.machine().map_or(0, Pet::master_clock)),
            "cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded<'a>(runtime: &'a PetRuntime, path: &str) -> Result<&'a Pet, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "ROMs not loaded",
        })
}
