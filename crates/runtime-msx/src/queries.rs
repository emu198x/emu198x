//! Family-owned query surface for the MSX1 runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_msx::Msx;
use serde_json::json;

use crate::runtime::MsxRuntime;

/// Paths the MSX1 runtime answers via `query()`.
pub(crate) const MSX_QUERY_PATHS: &[&str] = &[
    "bios.loaded",
    "cartridge.cart1.loaded",
    "cartridge.cart2.loaded",
    "cpu.pc",
    "cpu.sp",
    "cpu.tstates",
    "machine.frame_count",
    "machine.region",
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
            "vdp.scanline" => json!(loaded_machine(machine, path)?.vdp().scanline()),
            _ => return Ok(None),
        };

        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded_machine<'a>(runtime: &'a MsxRuntime, path: &str) -> Result<&'a Msx, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "BIOS not loaded — load firmware first",
        })
}
