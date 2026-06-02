//! Family-owned query surface for the BBC Micro runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_acorn_bbc_micro::BbcMicro;
use serde_json::json;

use crate::runtime::BbcMicroRuntime;

pub(crate) const BBC_QUERY_PATHS: &[&str] = &[
    "bbc.cpu.cycles",
    "bbc.cpu.pc",
    "bbc.firmware.loaded",
    "bbc.machine.frame_count",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BbcMicroSessionQueryProvider;

impl SessionQueryProvider<BbcMicroRuntime> for BbcMicroSessionQueryProvider {
    fn query_paths(&self, _machine: &BbcMicroRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = BBC_QUERY_PATHS
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
        machine: &BbcMicroRuntime,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "bbc.firmware.loaded" => json!(machine.machine().is_some()),
            "bbc.machine.frame_count" => json!(machine.machine().map_or(0, BbcMicro::frame_count)),
            "bbc.cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            "bbc.cpu.cycles" => json!(loaded(machine, path)?.cpu_cycles()),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded<'a>(runtime: &'a BbcMicroRuntime, path: &str) -> Result<&'a BbcMicro, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "MOS ROM not loaded",
        })
}
