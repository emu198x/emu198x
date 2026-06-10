//! Family-owned query surface for the Atari 5200 runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_atari_5200::Atari5200;
use serde_json::json;

use crate::runtime::Atari5200Runtime;

pub(crate) const A5200_QUERY_PATHS: &[&str] = &[
    "bios.loaded",
    "cartridge.loaded",
    "cpu.pc",
    "machine.frame_count",
    "machine.region",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Atari5200SessionQueryProvider;

impl SessionQueryProvider<Atari5200Runtime> for Atari5200SessionQueryProvider {
    fn query_paths(&self, _machine: &Atari5200Runtime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = A5200_QUERY_PATHS
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
        machine: &Atari5200Runtime,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "bios.loaded" => json!(!machine.bios_bytes().is_empty()),
            "cartridge.loaded" => json!(machine.cart_bytes().is_some()),
            "machine.region" => json!(format!("{:?}", machine.model().region())),
            "machine.frame_count" => {
                json!(machine.machine().map_or(0, Atari5200::frame_count))
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

fn loaded<'a>(runtime: &'a Atari5200Runtime, path: &str) -> Result<&'a Atari5200, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "no cartridge loaded",
        })
}
