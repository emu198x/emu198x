//! Query surface shared by every machine in the Master System class.

use emu198x_shell::{MachineCore, QueryError, QueryResult, SessionQueryProvider};
use machine_sega_master_system::Sms;
use serde_json::json;

use crate::runtime::SmsRuntime;

pub(crate) const SMS_QUERY_PATHS: &[&str] = &[
    "cartridge.loaded",
    "cpu.pc",
    "cpu.tstates",
    "machine.frame_count",
    "machine.region",
    "machine.variant",
    "mapper",
    "mapper.control",
    "mapper.page0",
    "mapper.page1",
    "mapper.page2",
    "vdp",
    "vdp.framebuffer_height",
    "vdp.framebuffer_width",
    "vdp.scanline",
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
            "cartridge.loaded" => json!(machine.cart_bytes().is_some()),
            "machine.region" => json!(format!("{:?}", machine.profile().region)),
            "machine.variant" => json!(format!("{:?}", machine.variant())),
            "machine.frame_count" => json!(machine.machine().map_or(0, Sms::frame_count)),
            "cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),
            "cpu.tstates" => json!(loaded(machine, path)?.cpu_tstates()),

            // Sega VDP — grouped snapshot + leaves. `scanline` is the V
            // counter (the chip's scanline register).
            "vdp" => {
                let vdp = loaded(machine, path)?.vdp();
                json!({
                    "scanline": vdp.read_v_counter(),
                    "framebuffer_width": vdp.framebuffer_width(),
                    "framebuffer_height": vdp.framebuffer_height(),
                })
            }
            "vdp.scanline" => json!(loaded(machine, path)?.vdp().read_v_counter()),
            "vdp.framebuffer_width" => json!(loaded(machine, path)?.vdp().framebuffer_width()),
            "vdp.framebuffer_height" => json!(loaded(machine, path)?.vdp().framebuffer_height()),

            // Sega mapper — control + three bank-page selects.
            "mapper" => {
                let regs = loaded(machine, path)?.mapper_regs();
                json!({
                    "control": format!("${:02X}", regs[0]),
                    "page0": format!("${:02X}", regs[1]),
                    "page1": format!("${:02X}", regs[2]),
                    "page2": format!("${:02X}", regs[3]),
                })
            }
            "mapper.control" => json!(format!("${:02X}", loaded(machine, path)?.mapper_regs()[0])),
            "mapper.page0" => json!(format!("${:02X}", loaded(machine, path)?.mapper_regs()[1])),
            "mapper.page1" => json!(format!("${:02X}", loaded(machine, path)?.mapper_regs()[2])),
            "mapper.page2" => json!(format!("${:02X}", loaded(machine, path)?.mapper_regs()[3])),

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
