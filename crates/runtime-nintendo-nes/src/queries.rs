//! Family-owned query surface for the NES runtime.
//!
//! Splits the SessionQueryProvider impl out of `runtime.rs` so the
//! query path catalogue and the blargg result-block decoder have one
//! home. The provider itself is stateless (`NesSessionQueryProvider`);
//! all the lookup logic lives here.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_nintendo_nes::Nes;
use serde_json::json;

use crate::runtime::NesRuntime;

/// Every path the NES runtime answers via `query()`.
pub(crate) const NES_QUERY_PATHS: &[&str] = &[
    "cartridge.loaded",
    "cartridge.mapper",
    "cpu.nmi",
    "cpu.nmi_pending",
    "cpu.nmi_prev",
    "cpu.pc",
    "machine.frame_count",
    "machine.master_clock",
    "ppu.ctrl",
    "ppu.dot",
    "ppu.frame_odd",
    "ppu.mask",
    "ppu.nmi",
    "ppu.rendering_enabled",
    "ppu.scanline",
    "ppu.status",
    "test.blargg.signature",
    "test.blargg.status",
    "test.blargg.text",
    "test.blargg.valid",
];

const BLARGG_STATUS_ADDR: u16 = 0x6000;
const BLARGG_SIGNATURE_ADDR: u16 = 0x6001;
const BLARGG_TEXT_ADDR: u16 = 0x6004;
const BLARGG_SIGNATURE: [u8; 3] = [0xDE, 0xB0, 0x61];
const BLARGG_MAX_TEXT_BYTES: u16 = 0x2000 - 4;

/// NES-family query provider layered above the shared shell surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NesSessionQueryProvider;

impl SessionQueryProvider<NesRuntime> for NesSessionQueryProvider {
    fn query_paths(&self, _machine: &NesRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = NES_QUERY_PATHS
            .iter()
            .copied()
            .filter(|path| prefix.is_none_or(|prefix| path.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(&self, machine: &NesRuntime, path: &str) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "cartridge.loaded" => json!(machine.machine().is_some()),
            "cartridge.mapper" => json!(machine.cartridge_mapper()),
            "machine.frame_count" => {
                json!(machine.machine().map_or(0, Nes::frame_count))
            }
            "machine.master_clock" => {
                json!(machine.machine().map_or(0, Nes::master_clock))
            }
            "cpu.pc" => json!(
                machine
                    .machine()
                    .ok_or_else(|| QueryError::UnavailablePath {
                        path: path.to_owned(),
                        reason: "no cartridge is loaded",
                    })?
                    .cpu
                    .regs
                    .pc
            ),
            "ppu.scanline" => json!(
                machine
                    .machine()
                    .ok_or_else(|| QueryError::UnavailablePath {
                        path: path.to_owned(),
                        reason: "no cartridge is loaded",
                    })?
                    .ppu
                    .scanline()
            ),
            "ppu.dot" => json!(
                machine
                    .machine()
                    .ok_or_else(|| QueryError::UnavailablePath {
                        path: path.to_owned(),
                        reason: "no cartridge is loaded",
                    })?
                    .ppu
                    .dot()
            ),
            "cpu.nmi" => json!(loaded_machine(machine, path)?.cpu.nmi),
            "cpu.nmi_pending" => json!(loaded_machine(machine, path)?.cpu.pending_nmi()),
            "cpu.nmi_prev" => json!(loaded_machine(machine, path)?.cpu.nmi_prev()),
            "ppu.ctrl" => json!(loaded_machine(machine, path)?.ppu.ctrl()),
            "ppu.mask" => json!(loaded_machine(machine, path)?.ppu.mask()),
            "ppu.status" => json!(loaded_machine(machine, path)?.ppu.status()),
            "ppu.frame_odd" => json!(loaded_machine(machine, path)?.ppu.frame_odd()),
            "ppu.rendering_enabled" => {
                json!(loaded_machine(machine, path)?.ppu.mask() & 0x18 != 0)
            }
            "ppu.nmi" => json!(loaded_machine(machine, path)?.ppu.nmi),
            "test.blargg.status" => {
                json!(loaded_machine(machine, path)?.peek(BLARGG_STATUS_ADDR))
            }
            "test.blargg.signature" => {
                json!(blargg_signature(loaded_machine(machine, path)?))
            }
            "test.blargg.valid" => {
                json!(blargg_signature(loaded_machine(machine, path)?) == BLARGG_SIGNATURE)
            }
            "test.blargg.text" => {
                json!(blargg_text(loaded_machine(machine, path)?))
            }
            _ => return Ok(None),
        };

        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded_machine<'a>(runtime: &'a NesRuntime, path: &str) -> Result<&'a Nes, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "no cartridge is loaded",
        })
}

fn blargg_signature(machine: &Nes) -> [u8; 3] {
    [
        machine.peek(BLARGG_SIGNATURE_ADDR),
        machine.peek(BLARGG_SIGNATURE_ADDR + 1),
        machine.peek(BLARGG_SIGNATURE_ADDR + 2),
    ]
}

fn blargg_text(machine: &Nes) -> String {
    let mut text = String::new();
    for offset in 0..BLARGG_MAX_TEXT_BYTES {
        let byte = machine.peek(BLARGG_TEXT_ADDR + offset);
        if byte == 0 {
            break;
        }
        text.push(match byte {
            b'\n' | b'\r' | b'\t' => char::from(byte),
            0x20..=0x7E => char::from(byte),
            _ => '.',
        });
    }
    text
}
