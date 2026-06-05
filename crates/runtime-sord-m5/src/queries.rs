//! Family-owned query surface for the Sord M5 runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_sord_m5::SordM5;
use serde_json::json;

use crate::runtime::M5Runtime;

pub(crate) const M5_QUERY_PATHS: &[&str] = &[
    "m5.cartridge.loaded",
    "m5.cpu.pc",
    "m5.cpu.tstates",
    "m5.firmware.loaded",
    "m5.input.joystick",
    "m5.machine.frame_count",
    "m5.machine.region",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct M5SessionQueryProvider;

impl SessionQueryProvider<M5Runtime> for M5SessionQueryProvider {
    fn query_paths(&self, _machine: &M5Runtime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = M5_QUERY_PATHS
            .iter()
            .copied()
            .filter(|p| prefix.is_none_or(|prefix| p.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(&self, machine: &M5Runtime, path: &str) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "m5.firmware.loaded" => json!(machine.machine().is_some()),
            "m5.cartridge.loaded" => json!(machine.cart_bytes().is_some()),
            "m5.machine.region" => json!(format!("{:?}", machine.model().region())),
            "m5.machine.frame_count" => json!(machine.machine().map_or(0, SordM5::frame_count)),
            "m5.cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            "m5.cpu.tstates" => json!(loaded(machine, path)?.cpu_tstates()),
            // Effective JOY byte ($37, active-high): P1 directions in the low
            // nibble (right=bit0, up=1, left=2, down=3), P2 in the high nibble.
            // Verifies host joystick input reached the chip.
            "m5.input.joystick" => json!(loaded(machine, path)?.joystick_byte()),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded<'a>(runtime: &'a M5Runtime, path: &str) -> Result<&'a SordM5, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "ROM not loaded",
        })
}
