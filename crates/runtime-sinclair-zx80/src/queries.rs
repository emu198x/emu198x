//! Family-owned query surface for the ZX80 runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_sinclair_zx80::Zx80;
use serde_json::json;

use crate::runtime::Zx80Runtime;

pub(crate) const ZX80_QUERY_PATHS: &[&str] = &[
    "zx80.cpu.pc",
    "zx80.firmware.loaded",
    "zx80.machine.frame_count",
    "zx80.ram.bytes",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Zx80SessionQueryProvider;

impl SessionQueryProvider<Zx80Runtime> for Zx80SessionQueryProvider {
    fn query_paths(&self, _machine: &Zx80Runtime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = ZX80_QUERY_PATHS
            .iter()
            .copied()
            .filter(|p| prefix.is_none_or(|prefix| p.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(&self, machine: &Zx80Runtime, path: &str) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "zx80.firmware.loaded" => json!(machine.machine().is_some()),
            "zx80.ram.bytes" => json!(machine.ram_bytes()),
            "zx80.machine.frame_count" => json!(machine.machine().map_or(0, Zx80::frame_count)),
            "zx80.cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded<'a>(runtime: &'a Zx80Runtime, path: &str) -> Result<&'a Zx80, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "ROM not loaded",
        })
}
