//! Family-owned query surface for the Atari 7800 runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_atari_7800::Atari7800;
use serde_json::json;

use crate::runtime::Atari7800Runtime;

pub(crate) const A7800_QUERY_PATHS: &[&str] = &[
    "atari7800.cartridge.loaded",
    "atari7800.cpu.pc",
    "atari7800.machine.frame_count",
    "atari7800.machine.region",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Atari7800SessionQueryProvider;

impl SessionQueryProvider<Atari7800Runtime> for Atari7800SessionQueryProvider {
    fn query_paths(&self, _machine: &Atari7800Runtime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = A7800_QUERY_PATHS
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
        machine: &Atari7800Runtime,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "atari7800.cartridge.loaded" => json!(machine.cart_bytes().is_some()),
            "atari7800.machine.region" => json!(format!("{:?}", machine.model().region())),
            "atari7800.machine.frame_count" => {
                json!(machine.machine().map_or(0, Atari7800::frame_count))
            }
            "atari7800.cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded<'a>(runtime: &'a Atari7800Runtime, path: &str) -> Result<&'a Atari7800, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "no cartridge loaded",
        })
}
