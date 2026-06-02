//! Family-owned query surface for the SMS runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_sega_master_system::Sms;
use serde_json::json;

use crate::runtime::SmsRuntime;

pub(crate) const SMS_QUERY_PATHS: &[&str] = &[
    "sms.cartridge.loaded",
    "sms.cpu.pc",
    "sms.cpu.tstates",
    "sms.machine.frame_count",
    "sms.machine.region",
    "sms.machine.variant",
    "sms.vdp.scanline",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SmsSessionQueryProvider;

impl SessionQueryProvider<SmsRuntime> for SmsSessionQueryProvider {
    fn query_paths(&self, _machine: &SmsRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = SMS_QUERY_PATHS
            .iter()
            .copied()
            .filter(|p| prefix.is_none_or(|prefix| p.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(&self, machine: &SmsRuntime, path: &str) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "sms.cartridge.loaded" => json!(machine.cart_bytes().is_some()),
            "sms.machine.region" => json!(format!("{:?}", machine.model().region())),
            "sms.machine.variant" => json!(format!("{:?}", machine.model())),
            "sms.machine.frame_count" => json!(machine.machine().map_or(0, Sms::frame_count)),
            "sms.cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            "sms.cpu.tstates" => json!(loaded(machine, path)?.cpu_tstates()),
            "sms.vdp.scanline" => json!(loaded(machine, path)?.vdp().read_v_counter()),
            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded<'a>(runtime: &'a SmsRuntime, path: &str) -> Result<&'a Sms, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "no cartridge loaded",
        })
}
