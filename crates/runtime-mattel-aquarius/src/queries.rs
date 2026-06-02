//! Family-owned query surface for the Aquarius runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_mattel_aquarius::Aquarius;
use serde_json::json;

use crate::runtime::AquariusRuntime;

pub(crate) const AQUARIUS_QUERY_PATHS: &[&str] = &[
    "aquarius.bios.loaded",
    "aquarius.cartridge.loaded",
    "aquarius.cpu.pc",
    "aquarius.cpu.tstates",
    "aquarius.expansion.kb",
    "aquarius.machine.frame_count",
    "aquarius.speaker.bit",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AquariusSessionQueryProvider;

impl SessionQueryProvider<AquariusRuntime> for AquariusSessionQueryProvider {
    fn query_paths(&self, _machine: &AquariusRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = AQUARIUS_QUERY_PATHS
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
        machine: &AquariusRuntime,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "aquarius.bios.loaded" => json!(machine.machine().is_some()),
            "aquarius.cartridge.loaded" => json!(machine.cart_bytes().is_some()),
            "aquarius.expansion.kb" => json!(machine.expansion_kb()),
            "aquarius.machine.frame_count" => {
                json!(machine.machine().map_or(0, Aquarius::frame_count))
            }
            "aquarius.cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            "aquarius.cpu.tstates" => json!(loaded(machine, path)?.cpu_tstates()),
            "aquarius.speaker.bit" => json!(loaded(machine, path)?.speaker_bit()),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded<'a>(runtime: &'a AquariusRuntime, path: &str) -> Result<&'a Aquarius, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "BIOS not loaded",
        })
}
