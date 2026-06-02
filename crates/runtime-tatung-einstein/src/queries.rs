//! Family-owned query surface for the Einstein runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_tatung_einstein::Einstein;
use serde_json::json;

use crate::runtime::EinsteinRuntime;

pub(crate) const EINSTEIN_QUERY_PATHS: &[&str] = &[
    "einstein.cpu.pc",
    "einstein.cpu.tstates",
    "einstein.firmware.loaded",
    "einstein.machine.frame_count",
    "einstein.machine.rom_paged_in",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EinsteinSessionQueryProvider;

impl SessionQueryProvider<EinsteinRuntime> for EinsteinSessionQueryProvider {
    fn query_paths(&self, _machine: &EinsteinRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = EINSTEIN_QUERY_PATHS
            .iter()
            .copied()
            .filter(|p| prefix.is_none_or(|prefix| p.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(
        &self,
        machine: &EinsteinRuntime,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "einstein.firmware.loaded" => json!(machine.machine().is_some()),
            "einstein.machine.frame_count" => json!(machine.machine().map_or(0, Einstein::frame_count)),
            "einstein.machine.rom_paged_in" => json!(machine.machine().is_some_and(Einstein::rom_paged_in)),
            "einstein.cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            "einstein.cpu.tstates" => json!(loaded(machine, path)?.cpu_tstates()),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded<'a>(runtime: &'a EinsteinRuntime, path: &str) -> Result<&'a Einstein, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "MOS ROM not loaded",
        })
}
