//! Family-owned query surface for the Oric runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_oric_atmos::OricAtmos;
use serde_json::json;

use crate::runtime::OricRuntime;

pub(crate) const ORIC_QUERY_PATHS: &[&str] = &[
    "oric.bios.loaded",
    "oric.cpu.pc",
    "oric.cpu.cycles",
    "oric.machine.frame_count",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OricSessionQueryProvider;

impl SessionQueryProvider<OricRuntime> for OricSessionQueryProvider {
    fn query_paths(&self, _machine: &OricRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = ORIC_QUERY_PATHS
            .iter()
            .copied()
            .filter(|p| prefix.is_none_or(|prefix| p.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(&self, machine: &OricRuntime, path: &str) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "oric.bios.loaded" => json!(machine.machine().is_some()),
            "oric.machine.frame_count" => {
                json!(machine.machine().map_or(0, OricAtmos::frame_count))
            }
            "oric.cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            "oric.cpu.cycles" => json!(loaded(machine, path)?.cpu_cycles()),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded<'a>(runtime: &'a OricRuntime, path: &str) -> Result<&'a OricAtmos, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "ROM not loaded",
        })
}
