//! Family-owned query surface for the Atari 800XL runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_atari_800xl::Atari800xl;
use serde_json::json;

use crate::runtime::Atari800xlRuntime;

pub(crate) const A800XL_QUERY_PATHS: &[&str] = &[
    "atari800xl.basic.enabled",
    "atari800xl.basic.loaded",
    "atari800xl.cartridge.loaded",
    "atari800xl.cpu.pc",
    "atari800xl.machine.frame_count",
    "atari800xl.os.loaded",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Atari800xlSessionQueryProvider;

impl SessionQueryProvider<Atari800xlRuntime> for Atari800xlSessionQueryProvider {
    fn query_paths(&self, _machine: &Atari800xlRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = A800XL_QUERY_PATHS
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
        machine: &Atari800xlRuntime,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "atari800xl.os.loaded" => json!(machine.os_bytes().is_some()),
            "atari800xl.basic.loaded" => json!(machine.basic_bytes().is_some()),
            "atari800xl.basic.enabled" => json!(machine.basic_enabled()),
            "atari800xl.cartridge.loaded" => json!(machine.cart_bytes().is_some()),
            "atari800xl.machine.frame_count" => {
                json!(machine.machine().map_or(0, Atari800xl::frame_count))
            }
            "atari800xl.cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded<'a>(runtime: &'a Atari800xlRuntime, path: &str) -> Result<&'a Atari800xl, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "machine not yet constructed (need at least OS ROM or cartridge)",
        })
}
