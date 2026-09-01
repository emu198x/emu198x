//! Family-owned query surface for the Atari 800XL runtime.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_atari_800xl::Atari800xl;
use serde_json::{Value, json};

use crate::runtime::Atari800xlRuntime;

pub(crate) const A800XL_QUERY_PATHS: &[&str] = &[
    "antic",
    "antic.chactl",
    "antic.chbase",
    "antic.dlist",
    "antic.dmactl",
    "antic.hscrol",
    "antic.nmien",
    "antic.scan_line",
    "antic.vcount",
    "antic.vscrol",
    "basic.enabled",
    "basic.loaded",
    "cartridge.loaded",
    "cpu.pc",
    "disk.loaded",
    "gtia",
    "gtia.colbk",
    "gtia.colpf",
    "gtia.colpm",
    "gtia.consol",
    "gtia.gractl",
    "gtia.prior",
    "machine.frame_count",
    "os.loaded",
    "pia",
    "pia.cra",
    "pia.crb",
    "pia.ddra",
    "pia.ddrb",
    "pia.irq_pending",
    "pia.porta",
    "pia.portb",
    "program.loaded",
    "program.pending",
    "pokey",
    "pokey.audc",
    "pokey.audctl",
    "pokey.audf",
    "pokey.irqen",
    "pokey.irqst",
    "pokey.kbcode",
    "pokey.skctl",
    "pokey.skstat",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Atari800xlSessionQueryProvider;

impl SessionQueryProvider<Atari800xlRuntime> for Atari800xlSessionQueryProvider {
    fn query_paths(&self, _machine: &Atari800xlRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = A800XL_QUERY_PATHS
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
        machine: &Atari800xlRuntime,
        path: &str,
    ) -> Result<Option<QueryResult>, QueryError> {
        let value = match path {
            "os.loaded" => json!(machine.os_bytes().is_some()),
            "basic.loaded" => json!(machine.basic_bytes().is_some()),
            "basic.enabled" => json!(machine.basic_enabled()),
            "cartridge.loaded" => json!(machine.cart_bytes().is_some()),
            "disk.loaded" => json!(machine.disk_in_d1()),
            "program.loaded" => json!(machine.xex_bytes().is_some()),
            "program.pending" => json!(machine.xex_pending()),
            "machine.frame_count" => {
                json!(machine.machine().map_or(0, Atari800xl::frame_count))
            }
            "cpu.pc" => json!(loaded(machine, path)?.cpu().regs.pc),

            // Chip register snapshots: a grouped object per chip, or any one
            // field as a leaf. The bespoke query_antic / query_gtia /
            // query_pokey / query_pia tools folded into these.
            p if is_chip(p, "antic") => {
                let a = loaded(machine, path)?.antic();
                return Ok(chip_field(
                    path,
                    "antic",
                    json!({
                        "dmactl": hex8(a.dmactl_value()),
                        "nmien": hex8(a.nmien_value()),
                        "dlist": format!("${:04X}", a.dlist_value()),
                        "chbase": hex8(a.chbase_value()),
                        "chactl": hex8(a.chactl_value()),
                        "hscrol": hex8(a.hscrol_value()),
                        "vscrol": hex8(a.vscrol_value()),
                        "scan_line": a.scan_line(),
                        "vcount": hex8(a.vcount()),
                    }),
                ));
            }
            p if is_chip(p, "gtia") => {
                let g = loaded(machine, path)?.gtia();
                return Ok(chip_field(
                    path,
                    "gtia",
                    json!({
                        "colbk": hex8(g.colbk_value()),
                        "colpf": hex8s(g.colpf_values()),
                        "colpm": hex8s(g.colpm_values()),
                        "prior": hex8(g.prior_value()),
                        "gractl": hex8(g.gractl_value()),
                        "consol": hex8(g.console_switches()),
                    }),
                ));
            }
            p if is_chip(p, "pokey") => {
                let p2 = loaded(machine, path)?.pokey();
                return Ok(chip_field(
                    path,
                    "pokey",
                    json!({
                        "audf": hex8s(p2.audf()),
                        "audc": hex8s(p2.audc()),
                        "audctl": hex8(p2.audctl()),
                        "irqen": hex8(p2.irqen()),
                        "irqst": hex8(p2.irqst()),
                        "skctl": hex8(p2.skctl()),
                        "skstat": hex8(p2.skstat()),
                        "kbcode": hex8(p2.kbcode()),
                    }),
                ));
            }
            p if is_chip(p, "pia") => {
                let pia = loaded(machine, path)?.pia();
                return Ok(chip_field(
                    path,
                    "pia",
                    json!({
                        "porta": hex8(pia.port_a_output()),
                        "portb": hex8(pia.port_b_output()),
                        "ddra": hex8(pia.ddr_a()),
                        "ddrb": hex8(pia.ddr_b()),
                        "cra": hex8(pia.cra()),
                        "crb": hex8(pia.crb()),
                        "irq_pending": pia.irq_pending(),
                    }),
                ));
            }

            _ => return Ok(None),
        };
        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn hex8(v: u8) -> String {
    format!("${v:02X}")
}

fn hex8s(vs: [u8; 4]) -> Vec<String> {
    vs.iter().map(|&v| hex8(v)).collect()
}

/// True when `path` is the chip's grouped object (`gtia`) or one of its
/// leaves (`gtia.colbk`).
fn is_chip(path: &str, chip: &str) -> bool {
    path == chip
        || path
            .strip_prefix(chip)
            .is_some_and(|rest| rest.starts_with('.'))
}

/// Resolve a chip path against its snapshot object: the whole object for the
/// grouped path, a single field for a leaf, or `None` for an unknown leaf.
fn chip_field(path: &str, chip: &str, snapshot: Value) -> Option<QueryResult> {
    let value = if path == chip {
        snapshot
    } else {
        let field = path.strip_prefix(chip)?.strip_prefix('.')?;
        snapshot.get(field)?.clone()
    };
    Some(QueryResult {
        path: path.to_owned(),
        value,
    })
}

fn loaded<'a>(runtime: &'a Atari800xlRuntime, path: &str) -> Result<&'a Atari800xl, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "machine not yet constructed (need at least OS ROM or cartridge)",
        })
}
