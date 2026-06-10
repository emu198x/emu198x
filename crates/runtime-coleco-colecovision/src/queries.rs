//! Family-owned query surface for the ColecoVision runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_coleco_colecovision::ColecoVision;
use serde_json::json;

use crate::runtime::CvRuntime;

pub(crate) const CV_QUERY_PATHS: &[&str] = &[
    "bios.loaded",
    "cartridge.loaded",
    "cpu.cycles",
    "cpu.pc",
    "machine.frame_count",
    "machine.region",
    "vdp",
    "vdp.framebuffer_height",
    "vdp.framebuffer_width",
    "vdp.scanline",
];

/// ColecoVision query provider.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CvSessionQueryProvider;

impl SessionQueryProvider<CvRuntime> for CvSessionQueryProvider {
    fn query_paths(&self, _machine: &CvRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = CV_QUERY_PATHS
            .iter()
            .copied()
            .filter(|p| prefix.is_none_or(|prefix| p.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(&self, machine: &CvRuntime, path: &str) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "bios.loaded" => json!(machine.machine().is_some()),
            "cartridge.loaded" => json!(machine.cart_bytes().is_some()),
            "machine.region" => json!(format!("{:?}", machine.model().region())),
            "machine.frame_count" => {
                json!(machine.machine().map_or(0, ColecoVision::frame_count))
            }
            "cpu.cycles" => json!(loaded_machine(machine, path)?.cpu_cycles()),
            "cpu.pc" => json!(loaded_machine(machine, path)?.cpu().regs.pc),

            // TMS9918 VDP — grouped snapshot + leaves.
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

            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded_machine<'a>(runtime: &'a CvRuntime, path: &str) -> Result<&'a ColecoVision, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "BIOS not loaded",
        })
}
