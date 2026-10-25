//! Family-owned query surface for the Electron runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_acorn_electron::AcornElectron;
use serde_json::json;

use crate::runtime::ElectronRuntime;

pub(crate) const ELECTRON_QUERY_PATHS: &[&str] = &[
    "cpu.cycles",
    "cpu.pc",
    "firmware.loaded",
    "machine.frame_count",
    "plus3",
    "plus3.drive0.loaded",
    "plus3.drive1.loaded",
    "plus3.fdc.status",
    "plus3.fdc.track",
    "plus3.fitted",
    "ula",
    "ula.display_mode",
    "ula.framebuffer_height",
    "ula.framebuffer_width",
    "ula.irq",
    "ula.rom_page",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ElectronSessionQueryProvider;

impl SessionQueryProvider<ElectronRuntime> for ElectronSessionQueryProvider {
    fn query_paths(&self, _machine: &ElectronRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = ELECTRON_QUERY_PATHS
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
        machine: &ElectronRuntime,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "firmware.loaded" => json!(machine.machine().is_some()),
            "machine.frame_count" => {
                json!(machine.machine().map_or(0, AcornElectron::frame_count))
            }
            "cpu.cycles" => json!(loaded(machine, path)?.cpu_cycles()),
            "cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            // Electron ULA — grouped snapshot + leaves.
            "ula" => {
                let el = loaded(machine, path)?;
                json!({
                    "display_mode": el.display_mode(),
                    "irq": el.irq_asserted(),
                    "framebuffer_width": el.framebuffer_width(),
                    "framebuffer_height": el.framebuffer_height(),
                })
            }
            "ula.display_mode" => json!(loaded(machine, path)?.display_mode()),
            "ula.rom_page" => json!(loaded(machine, path)?.rom_page()),
            // The Plus 3 — `fitted` answers on any model; the rest need it.
            "plus3.fitted" => json!(machine.machine().is_some_and(|m| m.plus3().is_some())),
            "plus3" => {
                let p = plus3(machine, path)?;
                let fdc = p.fdc();
                json!({
                    "adfs": p.has_adfs(),
                    "latch": p.latch(),
                    "rom_latch": p.rom_latch(),
                    "drive": p.selected_drive(),
                    "side": p.side(),
                    "double_density": p.double_density(),
                    "in_reset": p.in_reset(),
                    "fdc": {
                        "status": fdc.peek_status(),
                        "track": fdc.track_register(),
                        "head_track": fdc.head_track(),
                        "intrq": fdc.intrq(),
                        "drq": fdc.drq(),
                    },
                    "drives": (0..2).map(|d| disk_json(p.disk(d))).collect::<Vec<_>>(),
                })
            }
            "plus3.drive0.loaded" => json!(plus3(machine, path)?.disk(0).is_some()),
            "plus3.drive1.loaded" => json!(plus3(machine, path)?.disk(1).is_some()),
            "plus3.fdc.status" => json!(plus3(machine, path)?.fdc().peek_status()),
            "plus3.fdc.track" => json!(plus3(machine, path)?.fdc().track_register()),
            "ula.irq" => json!(loaded(machine, path)?.irq_asserted()),
            "ula.framebuffer_width" => json!(loaded(machine, path)?.framebuffer_width()),
            "ula.framebuffer_height" => json!(loaded(machine, path)?.framebuffer_height()),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

/// A drive's disc as JSON: `null` when empty.
fn disk_json(disk: Option<&machine_acorn_electron::Disk>) -> serde_json::Value {
    disk.map_or(serde_json::Value::Null, |d| {
        json!({
            "tracks": d.tracks(),
            "sides": d.sides(),
            "write_protected": d.is_write_protected(),
            "modified": d.is_dirty(),
        })
    })
}

fn plus3<'a>(
    runtime: &'a ElectronRuntime,
    path: &str,
) -> Result<&'a machine_acorn_electron::Plus3, QueryError> {
    loaded(runtime, path)?
        .plus3()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "no Plus 3 fitted (choose the acorn-electron-plus3 model)",
        })
}

fn loaded<'a>(runtime: &'a ElectronRuntime, path: &str) -> Result<&'a AcornElectron, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "OS / BASIC ROMs not loaded",
        })
}
