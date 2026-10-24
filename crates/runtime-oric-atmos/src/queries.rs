//! Family-owned query surface for the Oric runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_oric_atmos::OricAtmos;
use serde_json::json;

use crate::runtime::OricRuntime;

pub(crate) const ORIC_QUERY_PATHS: &[&str] = &[
    "acia.baud",
    "acia.command",
    "acia.control",
    "acia.irq",
    "acia.status",
    "bios.loaded",
    "cpu.pc",
    "cpu.cycles",
    "machine.frame_count",
    "serial.input_pending",
    "serial.present",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OricSessionQueryProvider;

impl SessionQueryProvider<OricRuntime> for OricSessionQueryProvider {
    fn query_paths(&self, _machine: &OricRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = ORIC_QUERY_PATHS
            .iter()
            .copied()
            .filter(|p| prefix.is_none_or(|prefix| p.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(&self, machine: &OricRuntime, path: &str) -> Result<Option<QueryResult>, QueryError> {
        let acia = machine.machine().and_then(OricAtmos::acia_registers);
        let value = match path {
            "acia.baud" => json!(machine.machine().and_then(OricAtmos::acia_baud_rate)),
            "acia.command" => json!(acia.map(|acia| acia.command)),
            "acia.control" => json!(acia.map(|acia| acia.control)),
            "acia.irq" => json!(acia.map(|acia| acia.irq)),
            "acia.status" => json!(acia.map(|acia| acia.status)),
            "serial.present" => json!(machine.serial_card()),
            "serial.input_pending" => {
                json!(machine.machine().map_or(0, OricAtmos::serial_input_pending))
            }
            "bios.loaded" => json!(machine.machine().is_some()),
            "machine.frame_count" => {
                json!(machine.machine().map_or(0, OricAtmos::frame_count))
            }
            "cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            "cpu.cycles" => json!(loaded(machine, path)?.cpu_cycles()),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded<'a>(runtime: &'a OricRuntime, path: &str) -> Result<&'a OricAtmos, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "ROM not loaded",
        })
}

#[cfg(test)]
mod tests {
    use emu198x_shell::SessionQueryProvider;
    use serde_json::{Value, json};

    use super::OricSessionQueryProvider;
    use crate::profiles::Model;
    use crate::runtime::OricRuntime;

    fn value(runtime: &OricRuntime, path: &str) -> Value {
        OricSessionQueryProvider
            .query(runtime, path)
            .expect("query")
            .expect("known path")
            .value
    }

    fn runtime() -> OricRuntime {
        let mut rom = vec![0xEA; 0x4000];
        rom[0x3FFC] = 0x00;
        rom[0x3FFD] = 0xC0;
        OricRuntime::new(Model::Atmos, rom).expect("runtime")
    }

    #[test]
    fn serial_queries_follow_the_card() {
        let mut runtime = runtime();
        assert_eq!(value(&runtime, "serial.present"), json!(false));
        assert_eq!(value(&runtime, "acia.status"), json!(null));
        assert!(!runtime.queue_serial_input(b"AB"));

        runtime.set_serial_card(true);
        assert_eq!(value(&runtime, "serial.present"), json!(true));
        assert_eq!(value(&runtime, "acia.status"), json!(0x10));
        assert_eq!(value(&runtime, "acia.command"), json!(0x02));
        assert_eq!(value(&runtime, "acia.control"), json!(0x00));
        assert_eq!(value(&runtime, "acia.irq"), json!(false));
        assert_eq!(value(&runtime, "acia.baud"), json!(null));
        assert!(runtime.queue_serial_input(b"AB"));
        assert_eq!(value(&runtime, "serial.input_pending"), json!(2));
        assert!(runtime.set_serial_modem_inputs(true, true, true));
        assert!(runtime.drain_serial_output().is_empty());
    }

    #[test]
    fn the_card_outlives_a_new_rom() {
        let mut runtime = runtime();
        runtime.set_serial_card(true);
        let mut rom = vec![0xEA; 0x4000];
        rom[0x3FFD] = 0xC0;
        runtime.set_bios(rom).expect("new ROM");
        assert_eq!(value(&runtime, "serial.present"), json!(true));
    }
}
