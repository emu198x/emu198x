//! Family-owned query surface for the SG-1000 runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_sega_sg_1000::Sg1000;
use serde_json::json;

use crate::runtime::Sg1000Runtime;

pub(crate) const SG1000_QUERY_PATHS: &[&str] = &[
    "sg1000.cartridge.loaded",
    "sg1000.cpu.pc",
    "sg1000.cpu.tstates",
    "sg1000.machine.frame_count",
    "sg1000.machine.region",
    "sg1000.vdp.scanline",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sg1000SessionQueryProvider;

impl SessionQueryProvider<Sg1000Runtime> for Sg1000SessionQueryProvider {
    fn query_paths(&self, _machine: &Sg1000Runtime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = SG1000_QUERY_PATHS
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
        machine: &Sg1000Runtime,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "sg1000.cartridge.loaded" => json!(machine.cart_bytes().is_some()),
            "sg1000.machine.region" => json!(format!("{:?}", machine.model().region())),
            "sg1000.machine.frame_count" => {
                json!(machine.machine().map_or(0, Sg1000::frame_count))
            }
            "sg1000.cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            "sg1000.cpu.tstates" => json!(loaded(machine, path)?.cpu_tstates()),
            "sg1000.vdp.scanline" => json!(loaded(machine, path)?.vdp().scanline()),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded<'a>(runtime: &'a Sg1000Runtime, path: &str) -> Result<&'a Sg1000, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "no cartridge loaded",
        })
}
