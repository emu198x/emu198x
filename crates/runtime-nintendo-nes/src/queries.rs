//! Family-owned query surface for the NES runtime.
//!
//! Splits the SessionQueryProvider impl out of `runtime.rs` so the
//! query path catalogue and the blargg result-block decoder have one
//! home. The provider itself is stateless (`NesSessionQueryProvider`);
//! all the lookup logic lives here.

use emu198x_shell::{QueryError, QueryResult, SessionQueryProvider};
use machine_nintendo_nes::Nes;
use serde_json::{Value, json};

use crate::runtime::NesRuntime;

/// Every path the NES runtime answers via `query()`.
///
/// The chip groups (`cpu`, `ppu`, `apu`, `mapper`) each resolve as a
/// grouped object *and* as one value per leaf — folded in from the old
/// bespoke `query_cpu` / `query_ppu` / `query_apu` / `query_mapper` MCP
/// tools (#456). Values are raw numbers, matching the rest of the fleet.
pub(crate) const NES_QUERY_PATHS: &[&str] = &[
    "cartridge.loaded",
    "cartridge.mapper",
    "cpu",
    "cpu.a",
    "cpu.addr_bus",
    "cpu.data_bus",
    "cpu.data_in",
    "cpu.flags",
    "cpu.halted",
    "cpu.instruction_complete",
    "cpu.instruction_cycle",
    "cpu.irq",
    "cpu.nmi",
    "cpu.nmi_pending",
    "cpu.nmi_prev",
    "cpu.p",
    "cpu.pc",
    "cpu.reset_phase",
    "cpu.rw",
    "cpu.sp",
    "cpu.sync",
    "cpu.total_cycles",
    "cpu.x",
    "cpu.y",
    "machine.frame_count",
    "machine.master_clock",
    "mapper",
    "mapper.irq_pending",
    "mapper.mapper_number",
    "mapper.mirroring",
    "apu",
    "apu.dmc",
    "apu.irq_pending",
    "ppu",
    "ppu.ctrl",
    "ppu.dot",
    "ppu.frame_odd",
    "ppu.mask",
    "ppu.nmi",
    "ppu.nmi_occurred",
    "ppu.nmi_output",
    "ppu.oam_addr",
    "ppu.ppu_clock",
    "ppu.pre_render_line",
    "ppu.rendering_enabled",
    "ppu.scanline",
    "ppu.status",
    "test.blargg.signature",
    "test.blargg.status",
    "test.blargg.text",
    "test.blargg.valid",
];

const BLARGG_STATUS_ADDR: u16 = 0x6000;
const BLARGG_SIGNATURE_ADDR: u16 = 0x6001;
const BLARGG_TEXT_ADDR: u16 = 0x6004;
const BLARGG_SIGNATURE: [u8; 3] = [0xDE, 0xB0, 0x61];
const BLARGG_MAX_TEXT_BYTES: u16 = 0x2000 - 4;

/// NES-family query provider layered above the shared shell surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NesSessionQueryProvider;

impl SessionQueryProvider<NesRuntime> for NesSessionQueryProvider {
    fn query_paths(&self, _machine: &NesRuntime, prefix: Option<&str>) -> Vec<String> {
        let mut paths: Vec<String> = NES_QUERY_PATHS
            .iter()
            .copied()
            .filter(|path| prefix.is_none_or(|prefix| path.starts_with(prefix)))
            .map(str::to_owned)
            .collect();
        paths.sort_unstable();
        paths
    }

    fn query(&self, machine: &NesRuntime, path: &str) -> Result<Option<QueryResult>, QueryError> {
        // Folded chip-register snapshots: a grouped object at the chip
        // name (`cpu` / `ppu` / `apu` / `mapper`) plus one value per leaf.
        // Every chip path routes through `loaded_machine`, so a blank
        // runtime reports `UnavailablePath` rather than a null value.
        if is_chip(path, "cpu") {
            return Ok(chip_field(
                path,
                "cpu",
                cpu_snapshot(loaded_machine(machine, path)?),
            ));
        }
        if is_chip(path, "ppu") {
            return Ok(chip_field(
                path,
                "ppu",
                ppu_snapshot(loaded_machine(machine, path)?),
            ));
        }
        if is_chip(path, "apu") {
            return Ok(chip_field(
                path,
                "apu",
                apu_snapshot(loaded_machine(machine, path)?),
            ));
        }
        if is_chip(path, "mapper") {
            let mapper_number = machine.cartridge_mapper();
            let snapshot = mapper_snapshot(mapper_number, loaded_machine(machine, path)?);
            return Ok(chip_field(path, "mapper", snapshot));
        }

        let value = match path {
            "cartridge.loaded" => json!(machine.machine().is_some()),
            "cartridge.mapper" => json!(machine.cartridge_mapper()),
            "machine.frame_count" => {
                json!(machine.machine().map_or(0, Nes::frame_count))
            }
            "machine.master_clock" => {
                json!(machine.machine().map_or(0, Nes::master_clock))
            }
            "test.blargg.status" => {
                json!(loaded_machine(machine, path)?.peek(BLARGG_STATUS_ADDR))
            }
            "test.blargg.signature" => {
                json!(blargg_signature(loaded_machine(machine, path)?))
            }
            "test.blargg.valid" => {
                json!(blargg_signature(loaded_machine(machine, path)?) == BLARGG_SIGNATURE)
            }
            "test.blargg.text" => {
                json!(blargg_text(loaded_machine(machine, path)?))
            }
            _ => return Ok(None),
        };

        Ok(Some(QueryResult {
            path: path.to_owned(),
            value,
        }))
    }
}

fn loaded_machine<'a>(runtime: &'a NesRuntime, path: &str) -> Result<&'a Nes, QueryError> {
    runtime
        .machine()
        .ok_or_else(|| QueryError::UnavailablePath {
            path: path.to_owned(),
            reason: "no cartridge is loaded",
        })
}

/// Does `path` address chip `chip` — either the bare group name or a
/// dotted leaf beneath it (`cpu`, `cpu.pc`)?
fn is_chip(path: &str, chip: &str) -> bool {
    path == chip
        || path
            .strip_prefix(chip)
            .is_some_and(|rest| rest.starts_with('.'))
}

/// Resolve a chip path against a built snapshot: the bare group name
/// returns the whole object, a `chip.field` leaf returns that field, and
/// an unknown sub-field returns `None` (an unknown path, not a null).
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

/// Full 6502 register + bus snapshot (folded from the old `query_cpu`).
fn cpu_snapshot(nes: &Nes) -> Value {
    let r = &nes.cpu.regs;
    json!({
        "pc": r.pc,
        "a": r.a,
        "x": r.x,
        "y": r.y,
        "sp": r.sp,
        "p": r.p,
        "flags": {
            "n": r.p & 0x80 != 0,
            "v": r.p & 0x40 != 0,
            "u": r.p & 0x20 != 0,
            "b": r.p & 0x10 != 0,
            "d": r.p & 0x08 != 0,
            "i": r.p & 0x04 != 0,
            "z": r.p & 0x02 != 0,
            "c": r.p & 0x01 != 0,
        },
        "addr_bus": nes.cpu.addr,
        "data_bus": nes.cpu.data,
        "data_in": nes.cpu.data_in,
        "rw": nes.cpu.rw,
        "sync": nes.cpu.sync,
        "nmi": nes.cpu.nmi,
        "irq": nes.cpu.irq,
        "nmi_pending": nes.cpu.pending_nmi(),
        "nmi_prev": nes.cpu.nmi_prev(),
        "instruction_complete": nes.cpu.instruction_complete(),
        "instruction_cycle": nes.cpu.instruction_cycle(),
        "total_cycles": nes.cpu.total_cycles,
        "reset_phase": nes.cpu.reset_phase,
        "halted": nes.cpu.halted,
    })
}

/// Full 2C02 PPU snapshot (folded from the old `query_ppu`).
fn ppu_snapshot(nes: &Nes) -> Value {
    let p = &nes.ppu;
    json!({
        "scanline": p.scanline(),
        "dot": p.dot(),
        "frame_odd": p.frame_odd(),
        "pre_render_line": p.pre_render_line(),
        "ppu_clock": p.ppu_clock(),
        "ctrl": p.ctrl(),
        "mask": p.mask(),
        "status": p.status(),
        "oam_addr": p.oam_addr(),
        "nmi_occurred": p.nmi_occurred(),
        "nmi_output": p.nmi_output(),
        "nmi": p.nmi,
        "rendering_enabled": (p.mask() & 0x18) != 0,
    })
}

/// 2A03 APU snapshot — frame-counter IRQ + DMC channel (folded from
/// the old `query_apu`).
fn apu_snapshot(nes: &Nes) -> Value {
    json!({
        "irq_pending": nes.apu.irq_pending(),
        "dmc": {
            "enabled": nes.apu.dmc.enabled(),
            "irq_enabled": nes.apu.dmc.irq_enabled(),
            "irq_flag": nes.apu.dmc.irq_flag,
            "output_level": nes.apu.dmc.output_level,
            "timer_period": nes.apu.dmc.timer_period,
            "sample_address": nes.apu.dmc.sample_address,
            "sample_length": nes.apu.dmc.sample_length,
            "current_address": nes.apu.dmc.current_address,
            "bytes_remaining": nes.apu.dmc.bytes_remaining,
            "shift_register": nes.apu.dmc.shift_register,
            "bits_remaining": nes.apu.dmc.bits_remaining,
            "silence_flag": nes.apu.dmc.silence_flag,
            "dma_pending": nes.apu.dmc.dma_pending,
        }
    })
}

/// Cartridge mapper snapshot (folded from the old `query_mapper`). The
/// mapper number comes from the runtime; mirroring + IRQ from the live
/// mapper.
fn mapper_snapshot(mapper_number: Option<u16>, nes: &Nes) -> Value {
    json!({
        "mapper_number": mapper_number,
        "mirroring": format!("{:?}", nes.mapper.mirroring()),
        "irq_pending": nes.mapper.irq_pending(),
    })
}

fn blargg_signature(machine: &Nes) -> [u8; 3] {
    [
        machine.peek(BLARGG_SIGNATURE_ADDR),
        machine.peek(BLARGG_SIGNATURE_ADDR + 1),
        machine.peek(BLARGG_SIGNATURE_ADDR + 2),
    ]
}

fn blargg_text(machine: &Nes) -> String {
    let mut text = String::new();
    for offset in 0..BLARGG_MAX_TEXT_BYTES {
        let byte = machine.peek(BLARGG_TEXT_ADDR + offset);
        if byte == 0 {
            break;
        }
        text.push(match byte {
            b'\n' | b'\r' | b'\t' => char::from(byte),
            0x20..=0x7E => char::from(byte),
            _ => '.',
        });
    }
    text
}
