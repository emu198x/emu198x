//! Family-owned query surface for the Acorn Atom runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_acorn_atom::AcornAtom;
use serde_json::json;

use crate::runtime::AtomRuntime;

pub(crate) const ATOM_QUERY_PATHS: &[&str] = &[
    "atom.bios.loaded",
    "atom.cpu.pc",
    "atom.cpu.master_clock",
    "atom.machine.frame_count",
    "atom.machine.ram_bytes",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AtomSessionQueryProvider;

impl SessionQueryProvider<AtomRuntime> for AtomSessionQueryProvider {
    fn query_paths(&self, _machine: &AtomRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = ATOM_QUERY_PATHS
            .iter()
            .copied()
            .filter(|p| prefix.is_none_or(|prefix| p.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(&self, machine: &AtomRuntime, path: &str) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "atom.bios.loaded" => json!(machine.machine().is_some()),
            "atom.machine.frame_count" => {
                json!(machine.machine().map_or(0, AcornAtom::frame_count))
            }
            "atom.machine.ram_bytes" => json!(machine.model().ram_bytes()),
            "atom.cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            "atom.cpu.master_clock" => json!(loaded(machine, path)?.master_clock()),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded<'a>(runtime: &'a AtomRuntime, path: &str) -> Result<&'a AcornAtom, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "ROM not loaded",
        })
}
