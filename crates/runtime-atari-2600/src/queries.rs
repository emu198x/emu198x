//! Family-owned query surface for the Atari 2600 runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_atari_2600::Atari2600;
use serde_json::json;

use crate::runtime::Atari2600Runtime;

pub(crate) const VCS_QUERY_PATHS: &[&str] = &[
    "vcs.cartridge.loaded",
    "vcs.cpu.pc",
    "vcs.input.inpt4",
    "vcs.input.inpt5",
    "vcs.input.swcha",
    "vcs.input.swchb",
    "vcs.machine.frame_count",
    "vcs.machine.master_clock",
    "vcs.machine.region",
    "vcs.tia.hpos",
    "vcs.tia.vpos",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Atari2600SessionQueryProvider;

impl SessionQueryProvider<Atari2600Runtime> for Atari2600SessionQueryProvider {
    fn query_paths(&self, _machine: &Atari2600Runtime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = VCS_QUERY_PATHS
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
        machine: &Atari2600Runtime,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "vcs.cartridge.loaded" => json!(machine.cart_bytes().is_some()),
            "vcs.machine.region" => json!(format!("{:?}", machine.model().region())),
            "vcs.machine.frame_count" => json!(machine.machine().map_or(0, Atari2600::frame_count)),
            "vcs.machine.master_clock" => {
                json!(machine.machine().map_or(0, Atari2600::master_clock))
            }
            "vcs.cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            // Effective input registers — verify host input reached the chips.
            // SWCHA/SWCHB are active-low (a 0 bit means a pressed direction or
            // switch); INPT4/5 bit 7 clear means the corresponding fire button
            // is held.
            "vcs.input.swcha" => json!(loaded(machine, path)?.riot().swcha()),
            "vcs.input.swchb" => json!(loaded(machine, path)?.riot().swchb()),
            "vcs.input.inpt4" => json!(loaded(machine, path)?.tia().read(0x0C)),
            "vcs.input.inpt5" => json!(loaded(machine, path)?.tia().read(0x0D)),
            "vcs.tia.hpos" => json!(loaded(machine, path)?.tia().hpos()),
            "vcs.tia.vpos" => json!(loaded(machine, path)?.tia().vpos()),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded<'a>(runtime: &'a Atari2600Runtime, path: &str) -> Result<&'a Atari2600, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "no cartridge loaded",
        })
}
