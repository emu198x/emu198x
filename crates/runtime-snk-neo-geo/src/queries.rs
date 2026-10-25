//! Query surface for the Neo Geo.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_snk_neo_geo::NeoGeo;
use serde_json::json;

use crate::runtime::NeoGeoRuntime;

pub(crate) const NEO_GEO_QUERY_PATHS: &[&str] = &[
    "cartridge.loaded",
    "cpu.pc",
    "lspc",
    "lspc.irq_level",
    "lspc.line",
    "machine.frame_count",
    "machine.mclk",
    "machine.variant",
    "sound",
    "sound.command",
    "sound.reply",
    "z80.pc",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NeoGeoSessionQueryProvider;

impl SessionQueryProvider<NeoGeoRuntime> for NeoGeoSessionQueryProvider {
    fn query_paths(&self, _machine: &NeoGeoRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = NEO_GEO_QUERY_PATHS
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
        machine: &NeoGeoRuntime,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "cartridge.loaded" => json!(machine.cartridge_loaded()),
            "machine.variant" => json!(machine.model().variant_id()),
            "machine.frame_count" => json!(machine.frames()),
            "machine.mclk" => json!(loaded(machine, path)?.mclk()),
            // The 68000's PC runs ahead by the prefetch; this is the
            // instruction itself.
            "cpu.pc" => json!(loaded(machine, path)?.cpu.instr_start_pc),
            "lspc" => {
                let m = loaded(machine, path)?;
                json!({
                    "line": m.lspc.line(),
                    "irq_level": m.lspc.irq_level(),
                })
            }
            "lspc.line" => json!(loaded(machine, path)?.lspc.line()),
            "lspc.irq_level" => json!(loaded(machine, path)?.lspc.irq_level()),
            "sound" => {
                let m = loaded(machine, path)?;
                let (command, pending) = m.sound_command();
                json!({
                    "command": command,
                    "pending": pending,
                    "reply": m.sound_reply(),
                })
            }
            "sound.command" => json!(loaded(machine, path)?.sound_command().0),
            "sound.reply" => json!(loaded(machine, path)?.sound_reply()),
            "z80.pc" => json!(loaded(machine, path)?.z80.regs.pc),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded<'a>(runtime: &'a NeoGeoRuntime, path: &str) -> Result<&'a NeoGeo, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "no BIOS and cartridge loaded",
        })
}
