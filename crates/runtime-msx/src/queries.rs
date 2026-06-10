//! Family-owned query surface for the MSX1 runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_msx::Msx;
use serde_json::json;

use crate::runtime::MsxRuntime;

/// Paths the MSX1 runtime answers via `query()`.
///
/// Chip state is exposed as fine-grained leaves (`vdp.scanline`, …) plus a
/// grouped object path per chip (`vdp`, `ay`, `ppi`) that returns the whole
/// snapshot in one call — the ergonomics the old bespoke `query_vdp` /
/// `query_psg` / `query_ppi` tools gave, now on the generic `query` surface.
/// The AY-3-8910 (MSX calls it the PSG) uses the canonical `ay.*` namespace
/// shared with the Spectrum.
pub(crate) const MSX_QUERY_PATHS: &[&str] = &[
    "ay",
    "ay.registers",
    "ay.selected_register",
    "bios.loaded",
    "cartridge.cart1.loaded",
    "cartridge.cart2.loaded",
    "cpu.pc",
    "cpu.sp",
    "cpu.tstates",
    "machine.frame_count",
    "machine.region",
    "ppi",
    "ppi.keyboard_row",
    "ppi.port_a",
    "vdp",
    "vdp.framebuffer_height",
    "vdp.framebuffer_width",
    "vdp.scanline",
];

/// MSX1-family query provider layered above the shared shell surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MsxSessionQueryProvider;

impl SessionQueryProvider<MsxRuntime> for MsxSessionQueryProvider {
    fn query_paths(&self, _machine: &MsxRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = MSX_QUERY_PATHS
            .iter()
            .copied()
            .filter(|path| prefix.is_none_or(|prefix| path.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(&self, machine: &MsxRuntime, path: &str) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "bios.loaded" => json!(machine.machine().is_some()),
            "cartridge.cart1.loaded" => json!(machine.cart1_bytes().is_some()),
            "cartridge.cart2.loaded" => json!(machine.cart2_bytes().is_some()),
            "machine.region" => json!(format!("{:?}", machine.model().region())),
            "machine.frame_count" => {
                json!(machine.machine().map_or(0, Msx::frame_count))
            }
            "cpu.pc" => json!(loaded_machine(machine, path)?.cpu().regs.pc),
            "cpu.sp" => json!(loaded_machine(machine, path)?.cpu().regs.sp),
            "cpu.tstates" => json!(loaded_machine(machine, path)?.cpu_tstates()),

            // VDP (TMS9918) — grouped snapshot + leaves.
            "vdp" => {
                let vdp = loaded_machine(machine, path)?.vdp();
                json!({
                    "scanline": vdp.scanline(),
                    "framebuffer_width": vdp.framebuffer_width(),
                    "framebuffer_height": vdp.framebuffer_height(),
                })
            }
            "vdp.scanline" => json!(loaded_machine(machine, path)?.vdp().scanline()),
            "vdp.framebuffer_width" => {
                json!(loaded_machine(machine, path)?.vdp().framebuffer_width())
            }
            "vdp.framebuffer_height" => {
                json!(loaded_machine(machine, path)?.vdp().framebuffer_height())
            }

            // AY-3-8910 PSG — grouped snapshot + leaves.
            "ay" => {
                let psg = loaded_machine(machine, path)?.psg();
                json!({
                    "selected_register": psg.selected_register(),
                    "registers": hex_bytes(psg.registers()),
                })
            }
            "ay.selected_register" => {
                json!(loaded_machine(machine, path)?.psg().selected_register())
            }
            "ay.registers" => json!(hex_bytes(loaded_machine(machine, path)?.psg().registers())),

            // Intel 8255 PPI — grouped snapshot + leaves.
            "ppi" => {
                let ppi = loaded_machine(machine, path)?.ppi();
                json!({
                    "port_a": format!("${:02X}", ppi.port_a),
                    "keyboard_row": ppi.keyboard_row(),
                })
            }
            "ppi.port_a" => {
                json!(format!(
                    "${:02X}",
                    loaded_machine(machine, path)?.ppi().port_a
                ))
            }
            "ppi.keyboard_row" => json!(loaded_machine(machine, path)?.ppi().keyboard_row()),

            _ => return Ok(None),
        };

        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

/// Format a register file as `$XX` hex strings, matching the shape the old
/// `query_psg` tool returned.
fn hex_bytes(bytes: &[u8]) -> Vec<String> {
    bytes.iter().map(|b| format!("${b:02X}")).collect()
}

fn loaded_machine<'a>(runtime: &'a MsxRuntime, path: &str) -> Result<&'a Msx, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "BIOS not loaded — load firmware first",
        })
}
