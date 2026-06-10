//! Family-owned query surface for the VIC-20 runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_commodore_vic_20::Vic20;
use serde_json::json;

use crate::runtime::Vic20Runtime;

pub(crate) const VIC20_QUERY_PATHS: &[&str] = &[
    "cpu.pc",
    "firmware.loaded",
    "machine.frame_count",
    "machine.master_clock",
    "machine.region",
    "ram.expansion_kb",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Vic20SessionQueryProvider;

impl SessionQueryProvider<Vic20Runtime> for Vic20SessionQueryProvider {
    fn query_paths(&self, _machine: &Vic20Runtime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = VIC20_QUERY_PATHS
            .iter()
            .copied()
            .filter(|p| prefix.is_none_or(|prefix| p.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(&self, machine: &Vic20Runtime, path: &str) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "firmware.loaded" => json!(machine.machine().is_some()),
            "ram.expansion_kb" => json!(machine.ram_expansion_kb()),
            "machine.region" => json!(format!("{:?}", machine.model().region())),
            "machine.frame_count" => json!(machine.machine().map_or(0, Vic20::frame_count)),
            "machine.master_clock" => {
                json!(machine.machine().map_or(0, Vic20::master_clock))
            }
            "cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded<'a>(runtime: &'a Vic20Runtime, path: &str) -> Result<&'a Vic20, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "ROMs not loaded",
        })
}
