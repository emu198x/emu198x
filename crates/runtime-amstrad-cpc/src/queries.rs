//! Family-owned query surface for the CPC runtime.

use amstrad_gate_array::VideoMode;
use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_amstrad_cpc::AmstradCpc;
use serde_json::json;

use crate::runtime::AmstradCpcRuntime;

pub(crate) const CPC_QUERY_PATHS: &[&str] = &[
    "cpu.pc",
    "cpu.tstates",
    "crtc.registers",
    "firmware.loaded",
    "gate_array.border",
    "gate_array.lower_rom_enabled",
    "gate_array.mode",
    "gate_array.palette",
    "gate_array.upper_rom_enabled",
    "machine.frame_count",
    "psg.registers",
    "tape.loaded",
    "tape.motor_on",
    // Position, so "did the tape drain" is observable rather than inferred.
    // Same names as every other machine with a deck; see
    // `common_tape::POSITION_QUERY_PATHS`. Kept in sorted order, which
    // `the_path_list_is_sorted_and_unique` enforces.
    "tape.progress",
    "tape.span_count",
    "tape.span_countdown",
    "tape.span_index",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AmstradCpcSessionQueryProvider;

impl SessionQueryProvider<AmstradCpcRuntime> for AmstradCpcSessionQueryProvider {
    fn query_paths(&self, _machine: &AmstradCpcRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = CPC_QUERY_PATHS
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
        machine: &AmstradCpcRuntime,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "firmware.loaded" => json!(machine.machine().is_some()),
            "machine.frame_count" => json!(machine.machine().map_or(0, AmstradCpc::frame_count)),
            "cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            "cpu.tstates" => json!(loaded(machine, path)?.cpu_tstates()),
            // Screen mode as its RMR number, which is how the firmware, the
            // `MODE` command and every CPC listing refer to it.
            "gate_array.mode" => json!(mode_number(loaded(machine, path)?.gate_array().mode())),
            "gate_array.border" => json!(format!(
                "#{:06X}",
                loaded(machine, path)?.gate_array().border_rgb() & 0x00FF_FFFF
            )),
            // The sixteen pens, as the hardware colour numbers they hold —
            // what a program wrote, rather than the RGB it resolves to.
            "gate_array.palette" => {
                let ga = loaded(machine, path)?.gate_array();
                json!((0..16).map(|pen| ga.pen_code(pen)).collect::<Vec<_>>())
            }
            "gate_array.lower_rom_enabled" => {
                json!(loaded(machine, path)?.gate_array().lower_rom_enabled())
            }
            "gate_array.upper_rom_enabled" => {
                json!(loaded(machine, path)?.gate_array().upper_rom_enabled())
            }
            "crtc.registers" => json!(loaded(machine, path)?.crtc().regs()),
            "psg.registers" => json!(loaded(machine, path)?.psg().registers()),
            // The CPC drives the cassette motor itself, so this reports what
            // the firmware asked for rather than a host setting.
            "tape.motor_on" => json!(loaded(machine, path)?.tape_motor_on()),
            "tape.loaded" => json!(loaded(machine, path)?.tape().has_tape()),
            "tape.span_index" => json!(loaded(machine, path)?.tape().span_index()),
            "tape.span_count" => json!(loaded(machine, path)?.tape().span_count()),
            "tape.span_countdown" => json!(loaded(machine, path)?.tape().span_countdown()),
            "tape.progress" => json!(loaded(machine, path)?.tape().progress()),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

/// The RMR bits a [`VideoMode`] came from.
const fn mode_number(mode: VideoMode) -> u8 {
    match mode {
        VideoMode::Mode0 => 0,
        VideoMode::Mode1 => 1,
        VideoMode::Mode2 => 2,
        VideoMode::Mode3 => 3,
    }
}

fn loaded<'a>(runtime: &'a AmstradCpcRuntime, path: &str) -> Result<&'a AmstradCpc, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "CPC firmware not loaded",
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::Model;

    #[test]
    fn every_advertised_path_answers_or_says_why() {
        // A path in the list that `query` does not match returns `Ok(None)`,
        // which reads to a caller as "no such path" — the list would be
        // advertising something that does not exist.
        let runtime = AmstradCpcRuntime::blank(Model::Cpc464);
        let provider = AmstradCpcSessionQueryProvider;
        for path in CPC_QUERY_PATHS {
            match provider.query(&runtime, path) {
                Ok(Some(_)) => {}
                Err(QueryError::UnavailablePath { .. }) => {}
                Ok(None) => panic!("{path} is advertised but unmatched"),
                Err(other) => panic!("{path}: unexpected error {other:?}"),
            }
        }
    }

    #[test]
    fn the_path_list_is_sorted_and_unique() {
        let mut sorted = CPC_QUERY_PATHS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, CPC_QUERY_PATHS);
    }

    #[test]
    fn a_prefix_narrows_the_listing() {
        let runtime = AmstradCpcRuntime::blank(Model::Cpc464);
        let paths = AmstradCpcSessionQueryProvider.query_paths(&runtime, Some("tape."));
        assert_eq!(
            paths,
            vec![
                "tape.loaded",
                "tape.motor_on",
                "tape.progress",
                "tape.span_count",
                "tape.span_countdown",
                "tape.span_index",
            ]
        );
    }
}
