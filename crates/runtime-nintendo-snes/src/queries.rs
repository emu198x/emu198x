//! Query surface for the Super NES.

use emu198x_shell::{MachineCore, QueryError, QueryResult, SessionQueryProvider};
use machine_nintendo_snes::Snes;
use serde_json::json;

use crate::runtime::SnesRuntime;

pub(crate) const SNES_QUERY_PATHS: &[&str] = &[
    "apu",
    "apu.ipl_enabled",
    "apu.original_ipl",
    "apu.pc",
    "apu.ports",
    "cartridge.loaded",
    "cartridge.map",
    "cartridge.title",
    "cpu.pc",
    "machine.frame_count",
    "machine.mclk",
    "machine.region",
    "machine.variant",
    "ppu",
    "ppu.hcounter",
    "ppu.interlaced",
    "ppu.vcounter",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SnesSessionQueryProvider;

impl SessionQueryProvider<SnesRuntime> for SnesSessionQueryProvider {
    fn query_paths(&self, _machine: &SnesRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = SNES_QUERY_PATHS
            .iter()
            .copied()
            .filter(|p| prefix.is_none_or(|prefix| p.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(&self, machine: &SnesRuntime, path: &str) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "cartridge.loaded" => json!(machine.cart_loaded()),
            "cartridge.map" => json!(format!("{:?}", loaded(machine, path)?.cart.map)),
            "cartridge.title" => json!(loaded(machine, path)?.cart.title),
            "machine.region" => json!(format!("{:?}", machine.profile().region)),
            "machine.variant" => json!(machine.model().variant_id()),
            "machine.frame_count" => json!(machine.machine().map_or(0, |m| m.ppu.frames())),
            "machine.mclk" => json!(loaded(machine, path)?.mclk()),
            "cpu.pc" => {
                let m = loaded(machine, path)?;
                json!(u32::from(m.cpu.regs.pbr) << 16 | u32::from(m.cpu.regs.pc))
            }
            "ppu" => {
                let m = loaded(machine, path)?;
                json!({
                    "vcounter": m.ppu.vcounter(),
                    "hcounter": m.ppu.hcounter(),
                    "interlaced": m.ppu.interlaced(),
                })
            }
            "ppu.vcounter" => json!(loaded(machine, path)?.ppu.vcounter()),
            "ppu.hcounter" => json!(loaded(machine, path)?.ppu.hcounter()),
            "ppu.interlaced" => json!(loaded(machine, path)?.ppu.interlaced()),
            "apu" => {
                let m = loaded(machine, path)?;
                json!({
                    "pc": m.apu.spc.regs.pc,
                    "ports": m.apu.to_cpu,
                    "ipl_enabled": m.apu.ipl_enabled(),
                    "original_ipl": machine.has_original_ipl(),
                })
            }
            "apu.pc" => json!(loaded(machine, path)?.apu.spc.regs.pc),
            "apu.ports" => json!(loaded(machine, path)?.apu.to_cpu),
            "apu.ipl_enabled" => json!(loaded(machine, path)?.apu.ipl_enabled()),
            "apu.original_ipl" => json!(machine.has_original_ipl()),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded<'a>(runtime: &'a SnesRuntime, path: &str) -> Result<&'a Snes, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "no cartridge loaded",
        })
}
