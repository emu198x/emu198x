//! Focused trace for the first BLIT interrupt request during KS3.1 boot.
//!
//! These tests are ignored by default because they require local Kickstart ROMs
//! and write JSON reports under `test_output/amiga/traces/`.

mod common;

use std::fs;
use std::path::PathBuf;

use machine_amiga::{
    Amiga, AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion, BlitterIrqDebugEvent,
};
use motorola_68000::bus::FunctionCode;
use serde::Serialize;

use common::load_rom;

const MAX_BOOT_TICKS: u64 = 120_000_000;
const MAX_CUSTOM_WRITES: usize = 96;
const MAX_INTREQ_CHANGES: usize = 24;

struct TraceSpec {
    model: AmigaModel,
    chipset: AmigaChipset,
    rom_path: &'static str,
    report_name: &'static str,
}

#[derive(Serialize)]
struct TraceReport {
    model: &'static str,
    rom_path: &'static str,
    first_blitter_irq_assert: Option<BlitterIrqDebugEvent>,
    blitter_irq_events: Vec<BlitterIrqDebugEvent>,
    intreq_changes: Vec<RegisterChange>,
    blitter_writes: Vec<CustomWriteEvent>,
}

#[derive(Serialize)]
struct RegisterChange {
    tick: u64,
    old_value: u16,
    new_value: u16,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    sr: u16,
}

#[derive(Serialize)]
struct CustomWriteEvent {
    tick: u64,
    register: &'static str,
    addr: u32,
    is_word: bool,
    raw_data: u16,
    effective_data: u16,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    sr: u16,
    intena: u16,
    intreq: u16,
}

fn report_path(report_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test_output/amiga/traces")
        .join(format!("{report_name}.json"))
}

fn write_report(report_name: &str, report: &TraceReport) {
    let path = report_path(report_name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create trace output directory");
    }
    let data = serde_json::to_vec_pretty(report).expect("serialize blitter IRQ trace report");
    fs::write(&path, data).expect("write blitter IRQ trace report");
    println!("blitter IRQ trace saved to {}", path.display());
}

fn model_name(model: AmigaModel) -> &'static str {
    match model {
        AmigaModel::A1000 => "a1000",
        AmigaModel::A500 => "a500",
        AmigaModel::A500Plus => "a500plus",
        AmigaModel::A600 => "a600",
        AmigaModel::A1200 => "a1200",
        AmigaModel::A2000 => "a2000",
        AmigaModel::A3000 => "a3000",
        AmigaModel::A4000 => "a4000",
    }
}

fn build_amiga(spec: &TraceSpec) -> Option<Amiga> {
    let rom = load_rom(spec.rom_path)?;
    Some(Amiga::new_with_config(AmigaConfig {
        model: spec.model,
        chipset: spec.chipset,
        region: AmigaRegion::Pal,
        kickstart: rom,
        slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
    }))
}

fn blitter_register_name(offset: u16) -> Option<&'static str> {
    match offset {
        0x040 => Some("BLTCON0"),
        0x042 => Some("BLTCON1"),
        0x044 => Some("BLTAFWM"),
        0x046 => Some("BLTALWM"),
        0x048 => Some("BLTCPTH"),
        0x04A => Some("BLTCPTL"),
        0x04C => Some("BLTBPTH"),
        0x04E => Some("BLTBPTL"),
        0x050 => Some("BLTAPTH"),
        0x052 => Some("BLTAPTL"),
        0x054 => Some("BLTDPTH"),
        0x056 => Some("BLTDPTL"),
        0x058 => Some("BLTSIZE"),
        0x05A => Some("BLTCON0L"),
        0x05C => Some("BLTSIZV"),
        0x05E => Some("BLTSIZH"),
        0x060 => Some("BLTCMOD"),
        0x062 => Some("BLTBMOD"),
        0x064 => Some("BLTAMOD"),
        0x066 => Some("BLTDMOD"),
        0x070 => Some("BLTCDAT"),
        0x072 => Some("BLTBDAT"),
        0x074 => Some("BLTADAT"),
        0x096 => Some("DMACON"),
        0x09A => Some("INTENA"),
        0x09C => Some("INTREQ"),
        _ => None,
    }
}

fn run_blitter_irq_trace(spec: &TraceSpec) {
    let Some(mut amiga) = build_amiga(spec) else {
        return;
    };

    let mut report = TraceReport {
        model: model_name(spec.model),
        rom_path: spec.rom_path,
        first_blitter_irq_assert: None,
        blitter_irq_events: Vec::new(),
        intreq_changes: Vec::new(),
        blitter_writes: Vec::new(),
    };

    let mut prev_bus_sig: Option<(u32, u8, bool, bool, Option<u16>)> = None;
    let mut prev_intreq = amiga.paula.intreq;

    for tick in 0..MAX_BOOT_TICKS {
        amiga.tick();

        if amiga.paula.intreq != prev_intreq && report.intreq_changes.len() < MAX_INTREQ_CHANGES {
            report.intreq_changes.push(RegisterChange {
                tick,
                old_value: prev_intreq,
                new_value: amiga.paula.intreq,
                pc: amiga.cpu.regs.pc,
                instr_start_pc: amiga.cpu.instr_start_pc,
                ir: amiga.cpu.ir,
                sr: amiga.cpu.regs.sr,
            });
            prev_intreq = amiga.paula.intreq;
        }

        let current_bus_sig = match &amiga.cpu.state {
            motorola_68000::cpu::State::BusCycle {
                addr,
                fc,
                is_read,
                is_word,
                data,
                ..
            } => Some((*addr, fc.bits(), *is_read, *is_word, *data)),
            _ => None,
        };

        if current_bus_sig != prev_bus_sig {
            if let Some((addr, fc_bits, is_read, is_word, data)) = current_bus_sig {
                let fc = match fc_bits {
                    7 => FunctionCode::InterruptAck,
                    6 => FunctionCode::SupervisorProgram,
                    5 => FunctionCode::SupervisorData,
                    2 => FunctionCode::UserProgram,
                    1 => FunctionCode::UserData,
                    _ => unreachable!("invalid function code"),
                };

                if fc != FunctionCode::InterruptAck
                    && !is_read
                    && report.blitter_writes.len() < MAX_CUSTOM_WRITES
                {
                    let addr24 = addr & 0x00FF_FFFF;
                    if (addr24 & 0xFFF000) == 0xDFF000 {
                        let offset = (addr24 & 0x01FE) as u16;
                        if let Some(register) = blitter_register_name(offset) {
                            let raw_data = data.unwrap_or(0);
                            let effective_data = if is_word {
                                raw_data
                            } else if addr24 & 1 == 0 {
                                raw_data << 8
                            } else {
                                raw_data & 0x00FF
                            };
                            report.blitter_writes.push(CustomWriteEvent {
                                tick,
                                register,
                                addr: addr24,
                                is_word,
                                raw_data,
                                effective_data,
                                pc: amiga.cpu.regs.pc,
                                instr_start_pc: amiga.cpu.instr_start_pc,
                                ir: amiga.cpu.ir,
                                sr: amiga.cpu.regs.sr,
                                intena: amiga.paula.intena,
                                intreq: amiga.paula.intreq,
                            });
                        }
                    }
                }
            }
        }
        prev_bus_sig = current_bus_sig;

        if let Some(first) = amiga.first_blitter_irq_assert() {
            report.first_blitter_irq_assert = Some(first);
            report.blitter_irq_events = amiga.blitter_irq_debug_events().to_vec();
            break;
        }
    }

    assert!(
        report.first_blitter_irq_assert.is_some(),
        "did not observe a BLIT interrupt assertion"
    );

    write_report(spec.report_name, &report);
}

#[test]
#[ignore]
fn trace_first_blitter_irq_a3000() {
    run_blitter_irq_trace(&TraceSpec {
        model: AmigaModel::A3000,
        chipset: AmigaChipset::Ecs,
        rom_path: "../../roms/kick31_40_068_a3000.rom",
        report_name: "blitter_irq_trace_kick31_a3000",
    });
}

#[test]
#[ignore]
fn trace_first_blitter_irq_a4000() {
    run_blitter_irq_trace(&TraceSpec {
        model: AmigaModel::A4000,
        chipset: AmigaChipset::Aga,
        rom_path: "../../roms/kick31_40_068_a4000.rom",
        report_name: "blitter_irq_trace_kick31_a4000",
    });
}
