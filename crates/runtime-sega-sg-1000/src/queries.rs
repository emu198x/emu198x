//! Family-owned query surface for the SG-1000 runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_sega_sg_1000::Sg1000;
use serde_json::json;

use crate::runtime::Sg1000Runtime;

pub(crate) const SG1000_QUERY_PATHS: &[&str] = &[
    "cartridge.loaded",
    "cpu.pc",
    "cpu.tstates",
    "machine.frame_count",
    "machine.region",
    "vdp",
    "vdp.framebuffer_height",
    "vdp.framebuffer_width",
    "vdp.scanline",
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
            "cartridge.loaded" => json!(machine.cart_bytes().is_some()),
            "machine.region" => json!(format!("{:?}", machine.model().region())),
            "machine.frame_count" => {
                json!(machine.machine().map_or(0, Sg1000::frame_count))
            }
            "cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            "cpu.tstates" => json!(loaded(machine, path)?.cpu_tstates()),

            // TMS9918 VDP — grouped snapshot + leaves.
            "vdp" => {
                let vdp = loaded(machine, path)?.vdp();
                json!({
                    "scanline": vdp.scanline(),
                    "framebuffer_width": vdp.framebuffer_width(),
                    "framebuffer_height": vdp.framebuffer_height(),
                })
            }
            "vdp.scanline" => json!(loaded(machine, path)?.vdp().scanline()),
            "vdp.framebuffer_width" => json!(loaded(machine, path)?.vdp().framebuffer_width()),
            "vdp.framebuffer_height" => json!(loaded(machine, path)?.vdp().framebuffer_height()),

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
