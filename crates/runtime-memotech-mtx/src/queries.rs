//! Family-owned query surface for the MTX runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_memotech_mtx::Mtx;
use serde_json::json;

use crate::runtime::MtxRuntime;

pub(crate) const MTX_QUERY_PATHS: &[&str] = &[
    "mtx.cpu.pc",
    "mtx.firmware.loaded",
    "mtx.machine.frame_count",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MtxSessionQueryProvider;

impl SessionQueryProvider<MtxRuntime> for MtxSessionQueryProvider {
    fn query_paths(&self, _machine: &MtxRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = MTX_QUERY_PATHS
            .iter()
            .copied()
            .filter(|p| prefix.is_none_or(|prefix| p.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(&self, machine: &MtxRuntime, path: &str) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "mtx.firmware.loaded" => json!(machine.machine().is_some()),
            "mtx.machine.frame_count" => json!(machine.machine().map_or(0, Mtx::frame_count)),
            "mtx.cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded<'a>(runtime: &'a MtxRuntime, path: &str) -> Result<&'a Mtx, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "ROM not loaded",
        })
}
