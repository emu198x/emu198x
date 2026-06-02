//! Family-owned query surface for the ZX81 runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_sinclair_zx81::Zx81;
use serde_json::json;

use crate::runtime::Zx81Runtime;

pub(crate) const ZX81_QUERY_PATHS: &[&str] = &[
    "zx81.cpu.pc",
    "zx81.firmware.loaded",
    "zx81.machine.frame_count",
    "zx81.ram.bytes",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Zx81SessionQueryProvider;

impl SessionQueryProvider<Zx81Runtime> for Zx81SessionQueryProvider {
    fn query_paths(&self, _machine: &Zx81Runtime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = ZX81_QUERY_PATHS
            .iter()
            .copied()
            .filter(|p| prefix.is_none_or(|prefix| p.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(&self, machine: &Zx81Runtime, path: &str) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "zx81.firmware.loaded" => json!(machine.machine().is_some()),
            "zx81.ram.bytes" => json!(machine.ram_bytes()),
            "zx81.machine.frame_count" => json!(machine.machine().map_or(0, Zx81::frame_count)),
            "zx81.cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded<'a>(runtime: &'a Zx81Runtime, path: &str) -> Result<&'a Zx81, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "ROM not loaded",
        })
}
