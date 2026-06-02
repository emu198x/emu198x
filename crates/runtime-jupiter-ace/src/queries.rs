//! Family-owned query surface for the Jupiter Ace runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_jupiter_ace::JupiterAce;
use serde_json::json;

use crate::runtime::JupiterAceRuntime;

pub(crate) const ACE_QUERY_PATHS: &[&str] = &[
    "ace.bios.loaded",
    "ace.cpu.pc",
    "ace.cpu.master_clock",
    "ace.machine.frame_count",
    "ace.machine.ram_kb",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JupiterAceSessionQueryProvider;

impl SessionQueryProvider<JupiterAceRuntime> for JupiterAceSessionQueryProvider {
    fn query_paths(&self, _machine: &JupiterAceRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = ACE_QUERY_PATHS
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
        machine: &JupiterAceRuntime,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "ace.bios.loaded" => json!(machine.machine().is_some()),
            "ace.machine.frame_count" => {
                json!(machine.machine().map_or(0, JupiterAce::frame_count))
            }
            "ace.machine.ram_kb" => json!(machine.model().ram_kb()),
            "ace.cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            "ace.cpu.master_clock" => json!(loaded(machine, path)?.master_clock()),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded<'a>(runtime: &'a JupiterAceRuntime, path: &str) -> Result<&'a JupiterAce, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "ROM not loaded",
        })
}
