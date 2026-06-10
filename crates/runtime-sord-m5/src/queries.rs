//! Family-owned query surface for the Sord M5 runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_sord_m5::SordM5;
use serde_json::{Value, json};

use crate::runtime::M5Runtime;

pub(crate) const M5_QUERY_PATHS: &[&str] = &[
    "cartridge.loaded",
    "cpu.pc",
    "cpu.tstates",
    "ctc",
    "ctc.interrupt",
    "ctc.vector_base",
    "firmware.loaded",
    "input.joystick",
    "machine.frame_count",
    "machine.region",
    "vdp",
    "vdp.registers",
    "vdp.scanline",
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
            "firmware.loaded" => json!(machine.machine().is_some()),
            "cartridge.loaded" => json!(machine.cart_bytes().is_some()),
            "machine.region" => json!(format!("{:?}", machine.model().region())),
            "machine.frame_count" => json!(machine.machine().map_or(0, SordM5::frame_count)),
            "cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            "cpu.tstates" => json!(loaded(machine, path)?.cpu_tstates()),
            // Effective JOY byte ($37, active-high): P1 directions in the low
            // nibble (right=bit0, up=1, left=2, down=3), P2 in the high nibble.
            // Verifies host joystick input reached the chip.
            "input.joystick" => json!(loaded(machine, path)?.joystick_byte()),

            // Z80 CTC — grouped snapshot (vector base, INT line, the four
            // channels) plus scalar leaves.
            "ctc" => {
                let ctc = loaded(machine, path)?.ctc();
                let channels: Vec<Value> = (0u8..4)
                    .map(|ch| {
                        json!({
                            "channel": ch,
                            "running": ctc.running(ch),
                            "counter_mode": ctc.counter_mode(ch),
                            "int_enabled": ctc.int_enabled(ch),
                            "counter": ctc.counter(ch),
                        })
                    })
                    .collect();
                json!({
                    "vector_base": format!("${:02X}", ctc.vector_base()),
                    "interrupt": ctc.interrupt(),
                    "channels": channels,
                })
            }
            "ctc.vector_base" => {
                json!(format!(
                    "${:02X}",
                    loaded(machine, path)?.ctc().vector_base()
                ))
            }
            "ctc.interrupt" => json!(loaded(machine, path)?.ctc().interrupt()),

            // TMS9918 VDP — grouped snapshot + leaves.
            "vdp" => {
                let vdp = loaded(machine, path)?.vdp();
                json!({
                    "registers": hex_regs(vdp.registers()),
                    "scanline": vdp.scanline(),
                })
            }
            "vdp.scanline" => json!(loaded(machine, path)?.vdp().scanline()),
            "vdp.registers" => json!(hex_regs(loaded(machine, path)?.vdp().registers())),

            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

/// Format a register file as `$XX` hex strings.
fn hex_regs(bytes: &[u8]) -> Vec<String> {
    bytes.iter().map(|b| format!("${b:02X}")).collect()
}

fn loaded<'a>(runtime: &'a M5Runtime, path: &str) -> Result<&'a SordM5, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "ROM not loaded",
        })
}
