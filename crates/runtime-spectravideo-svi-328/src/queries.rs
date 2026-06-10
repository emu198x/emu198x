//! Family-owned query surface for the SVI-328 runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_spectravideo_svi_328::Svi328;
use serde_json::json;

use crate::runtime::Svi328Runtime;

pub(crate) const SVI_QUERY_PATHS: &[&str] = &[
    "bios.loaded",
    "cartridge.loaded",
    "cpu.pc",
    "cpu.tstates",
    "machine.frame_count",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Svi328SessionQueryProvider;

impl SessionQueryProvider<Svi328Runtime> for Svi328SessionQueryProvider {
    fn query_paths(&self, _machine: &Svi328Runtime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = SVI_QUERY_PATHS
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
        machine: &Svi328Runtime,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "bios.loaded" => json!(machine.machine().is_some()),
            "cartridge.loaded" => json!(machine.cart_bytes().is_some()),
            "machine.frame_count" => {
                json!(machine.machine().map_or(0, Svi328::frame_count))
            }
            "cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            "cpu.tstates" => json!(loaded(machine, path)?.cpu_tstates()),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded<'a>(runtime: &'a Svi328Runtime, path: &str) -> Result<&'a Svi328, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "ROM not loaded",
        })
}
