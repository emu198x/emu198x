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
    "nes.cartridge.loaded",
    "nes.cartridge.mapper",
    "nes.cpu.pc",
    "nes.machine.frame_count",
    "nes.machine.master_clock",
    "nes.ppu.dot",
    "nes.ppu.scanline",
    "nes.test.blargg.signature",
    "nes.test.blargg.status",
    "nes.test.blargg.text",
    "nes.test.blargg.valid",
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
            "nes.cartridge.loaded" => json!(machine.machine().is_some()),
            "nes.cartridge.mapper" => json!(machine.cartridge_mapper()),
            "nes.machine.frame_count" => {
                json!(machine.machine().map_or(0, Nes::frame_count))
            }
            "nes.machine.master_clock" => {
                json!(machine.machine().map_or(0, Nes::master_clock))
            }
            "nes.cpu.pc" => json!(
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
            "nes.ppu.scanline" => json!(
                machine
                    .machine()
                    .ok_or_else(|| QueryError::UnavailablePath {
                        path: path.to_owned(),
                        reason: "no cartridge is loaded",
                    })?
                    .ppu
                    .scanline()
            ),
            "nes.ppu.dot" => json!(
                machine
                    .machine()
                    .ok_or_else(|| QueryError::UnavailablePath {
                        path: path.to_owned(),
                        reason: "no cartridge is loaded",
                    })?
                    .ppu
                    .dot()
            ),
            "nes.test.blargg.status" => {
                json!(loaded_machine(machine, path)?.peek(BLARGG_STATUS_ADDR))
            }
            "nes.test.blargg.signature" => {
                json!(blargg_signature(loaded_machine(machine, path)?))
            }
            "nes.test.blargg.valid" => {
                json!(blargg_signature(loaded_machine(machine, path)?) == BLARGG_SIGNATURE)
            }
            "nes.test.blargg.text" => {
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
