//! Query surface for the Mega Drive.

use crate::MegaDrive;
use emu198x_shell::{MachineCore, QueryError, QueryResult, SessionQueryProvider};
use serde_json::json;

use crate::runtime::MegaDriveRuntime;

pub(crate) const MEGA_DRIVE_QUERY_PATHS: &[&str] = &[
    "cartridge.loaded",
    "cpu.pc",
    "machine.frame_count",
    "machine.mclk",
    "machine.region",
    "machine.variant",
    "vdp",
    "vdp.frame_height",
    "vdp.frame_width",
    "vdp.hslot",
    "vdp.vcounter",
    "z80",
    "z80.bus_acknowledged",
    "z80.in_reset",
    "z80.pc",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MegaDriveSessionQueryProvider;

impl SessionQueryProvider<MegaDriveRuntime> for MegaDriveSessionQueryProvider {
    fn query_paths(&self, _machine: &MegaDriveRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = MEGA_DRIVE_QUERY_PATHS
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
        machine: &MegaDriveRuntime,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "cartridge.loaded" => json!(machine.cart_bytes().is_some()),
            "machine.region" => json!(format!("{:?}", machine.profile().region)),
            "machine.variant" => json!(machine.model().variant_id()),
            "machine.frame_count" => json!(machine.machine().map_or(0, |m| m.vdp().frames())),
            "machine.mclk" => json!(loaded(machine, path)?.mclk()),
            // The 68000's PC is two words ahead of the instruction it is
            // running (the prefetch); this is the instruction itself.
            "cpu.pc" => json!(loaded(machine, path)?.cpu().instr_start_pc),

            // VDP — grouped snapshot + leaves.
            "vdp" => {
                let m = loaded(machine, path)?;
                json!({
                    "vcounter": m.vdp().vcounter(),
                    "hslot": m.vdp().hslot(),
                    "frame_width": m.frame_width(),
                    "frame_height": m.frame_height(),
                })
            }
            "vdp.vcounter" => json!(loaded(machine, path)?.vdp().vcounter()),
            "vdp.hslot" => json!(loaded(machine, path)?.vdp().hslot()),
            "vdp.frame_width" => json!(loaded(machine, path)?.frame_width()),
            "vdp.frame_height" => json!(loaded(machine, path)?.frame_height()),

            // The sound CPU — grouped snapshot + leaves.
            "z80" => {
                let m = loaded(machine, path)?;
                json!({
                    "pc": m.z80().regs.pc,
                    "in_reset": m.z80_in_reset(),
                    "bus_acknowledged": m.z80_bus_acknowledged(),
                })
            }
            "z80.pc" => json!(loaded(machine, path)?.z80().regs.pc),
            "z80.in_reset" => json!(loaded(machine, path)?.z80_in_reset()),
            "z80.bus_acknowledged" => json!(loaded(machine, path)?.z80_bus_acknowledged()),

            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded<'a>(runtime: &'a MegaDriveRuntime, path: &str) -> Result<&'a MegaDrive, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "no cartridge loaded",
        })
}
