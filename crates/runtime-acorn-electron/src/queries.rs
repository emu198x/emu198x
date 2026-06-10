//! Family-owned query surface for the Electron runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_acorn_electron::AcornElectron;
use serde_json::json;

use crate::runtime::ElectronRuntime;

pub(crate) const ELECTRON_QUERY_PATHS: &[&str] = &[
    "cpu.cycles",
    "cpu.pc",
    "firmware.loaded",
    "machine.frame_count",
    "ula.display_mode",
    "ula.irq",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ElectronSessionQueryProvider;

impl SessionQueryProvider<ElectronRuntime> for ElectronSessionQueryProvider {
    fn query_paths(&self, _machine: &ElectronRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = ELECTRON_QUERY_PATHS
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
        machine: &ElectronRuntime,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "firmware.loaded" => json!(machine.machine().is_some()),
            "machine.frame_count" => {
                json!(machine.machine().map_or(0, AcornElectron::frame_count))
            }
            "cpu.cycles" => json!(loaded(machine, path)?.cpu_cycles()),
            "cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            "ula.display_mode" => json!(loaded(machine, path)?.display_mode()),
            "ula.irq" => json!(loaded(machine, path)?.irq_asserted()),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded<'a>(runtime: &'a ElectronRuntime, path: &str) -> Result<&'a AcornElectron, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "OS / BASIC ROMs not loaded",
        })
}
