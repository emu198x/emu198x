//! Query surface for the Atari ST.

use crate::machine::AtariSt;
use common_atari_st::{Frequency, Monitor};
use emu198x_shell::{MachineCore, QueryError, QueryResult, SessionQueryProvider};
use serde_json::{Value, json};

use crate::runtime::AtariStRuntime;

pub(crate) const ATARI_ST_QUERY_PATHS: &[&str] = &[
    "cpu.pc",
    "floppy",
    "floppy.a",
    "floppy.b",
    "machine.cycles",
    "machine.frame_count",
    "machine.monitor",
    "machine.region",
    "machine.variant",
    "tos.loaded",
    "tos.version",
    "video",
    "video.base",
    "video.frequency",
    "video.line",
    "video.resolution",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AtariStSessionQueryProvider;

impl SessionQueryProvider<AtariStRuntime> for AtariStSessionQueryProvider {
    fn query_paths(&self, _machine: &AtariStRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = ATARI_ST_QUERY_PATHS
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
        machine: &AtariStRuntime,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "tos.loaded" => json!(machine.tos().is_some()),
            // The TOS header's version word: $0104 for TOS 1.04, and
            // EmuTOS's own number for EmuTOS.
            "tos.version" => {
                let tos = machine.tos().ok_or_else(|| unavailable(path))?;
                json!(format!("{:02X}.{:02X}", tos[2], tos[3]))
            }
            "machine.region" => json!(format!("{:?}", machine.profile().region)),
            "machine.variant" => json!(machine.model().variant_id()),
            "machine.monitor" => json!(match machine.monitor() {
                Monitor::Colour => "colour",
                Monitor::Mono => "mono",
            }),
            "machine.frame_count" => json!(machine.machine().map_or(0, |m| m.glue().frames())),
            "machine.cycles" => json!(loaded(machine, path)?.cycles()),
            // The 68000's PC is two words ahead of the instruction it is
            // running (the prefetch); this is the instruction itself.
            "cpu.pc" => json!(loaded(machine, path)?.cpu().instr_start_pc),

            "video" => {
                let m = loaded(machine, path)?;
                json!({
                    "base": m.glue().video_base(),
                    "frequency": frequency(m),
                    "line": m.glue().position().0,
                    "resolution": resolution(m),
                })
            }
            "video.base" => json!(loaded(machine, path)?.glue().video_base()),
            "video.frequency" => frequency(loaded(machine, path)?),
            "video.line" => json!(loaded(machine, path)?.glue().position().0),
            "video.resolution" => resolution(loaded(machine, path)?),

            "floppy" => {
                let m = loaded(machine, path)?;
                json!({ "a": m.disk(0).is_some(), "b": m.disk(1).is_some() })
            }
            "floppy.a" => json!(loaded(machine, path)?.disk(0).is_some()),
            "floppy.b" => json!(loaded(machine, path)?.disk(1).is_some()),

            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

/// The frame rate the GLUE is generating, in whole hertz.
fn frequency(machine: &AtariSt) -> Value {
    json!(match machine.glue().frequency() {
        Frequency::Hz50 => 50,
        Frequency::Hz60 => 60,
        Frequency::Hz71 => 71,
    })
}

fn resolution(machine: &AtariSt) -> Value {
    json!(match machine.shifter().read_resolution() & 3 {
        0 => "low",
        1 => "medium",
        _ => "high",
    })
}

fn unavailable(path: &str) -> QueryError {
    QueryError::UnavailablePath {
        path: path.to_owned(),
        reason: "no TOS loaded",
    }
}

fn loaded<'a>(runtime: &'a AtariStRuntime, path: &str) -> Result<&'a AtariSt, QueryError> {
    runtime.machine().ok_or_else(|| unavailable(path))
}
