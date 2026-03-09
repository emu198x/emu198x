//! Focused IRQ3 trace for KS3.1 A3000/A4000 boot stalls.
//!
//! These tests are ignored by default because they require local Kickstart ROMs
//! and write JSON reports under `test_output/amiga/traces/`.

mod common;

use std::fs;
use std::path::PathBuf;

use machine_amiga::{Amiga, AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};
use motorola_68000::bus::FunctionCode;
use motorola_68000::cpu::State;
use serde::Serialize;

use common::load_rom;

const MAX_BOOT_TICKS: u64 = 120_000_000;
const POST_STOP_TRACE_TICKS: u64 = 2_000_000;
const MAX_IACK_EVENTS: usize = 8;
const MAX_PC_SAMPLES: usize = 96;
const MAX_CUSTOM_WRITES: usize = 32;

struct TraceSpec {
    model: AmigaModel,
    chipset: AmigaChipset,
    rom_path: &'static str,
    report_name: &'static str,
    stop_resume_pc: u32,
}

#[derive(Serialize)]
struct TraceReport {
    model: &'static str,
    rom_path: &'static str,
    stop_resume_pc: u32,
    stop_tick: Option<u64>,
    stop_state: Option<CpuSample>,
    iack_events: Vec<IackEvent>,
    custom_write_events: Vec<CustomWriteEvent>,
    intreq_changes: Vec<RegisterChange>,
    intena_changes: Vec<RegisterChange>,
    pc_samples: Vec<CpuSample>,
}

#[derive(Clone, Copy, Serialize)]
struct CpuSample {
    tick: u64,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    sr: u16,
    ssp: u32,
    state: &'static str,
    intena: u16,
    intreq: u16,
}

#[derive(Serialize)]
struct IackEvent {
    tick: u64,
    level: u8,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    sr: u16,
    intena: u16,
    intreq: u16,
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
    let data = serde_json::to_vec_pretty(report).expect("serialize trace report");
    fs::write(&path, data).expect("write trace report");
    println!("IRQ3 trace saved to {}", path.display());
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

fn state_name(state: &State) -> &'static str {
    match state {
        State::Idle => "idle",
        State::Internal { .. } => "internal",
        State::BusCycle { .. } => "bus",
        State::Halted => "halted",
        State::Stopped => "stopped",
    }
}

fn sample_cpu(amiga: &Amiga, tick: u64) -> CpuSample {
    CpuSample {
        tick,
        pc: amiga.cpu.regs.pc,
        instr_start_pc: amiga.cpu.instr_start_pc,
        ir: amiga.cpu.ir,
        sr: amiga.cpu.regs.sr,
        ssp: amiga.cpu.regs.ssp,
        state: state_name(&amiga.cpu.state),
        intena: amiga.paula.intena,
        intreq: amiga.paula.intreq,
    }
}

fn custom_register_name(offset: u16) -> Option<&'static str> {
    match offset {
        0x09A => Some("INTENA"),
        0x09C => Some("INTREQ"),
        _ => None,
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
    }))
}

fn run_irq3_trace(spec: &TraceSpec) {
    let Some(mut amiga) = build_amiga(spec) else {
        return;
    };

    let mut report = TraceReport {
        model: model_name(spec.model),
        rom_path: spec.rom_path,
        stop_resume_pc: spec.stop_resume_pc,
        stop_tick: None,
        stop_state: None,
        iack_events: Vec::new(),
        custom_write_events: Vec::new(),
        intreq_changes: Vec::new(),
        intena_changes: Vec::new(),
        pc_samples: Vec::new(),
    };

    let mut prev_bus_sig: Option<(u32, u8, bool, bool, Option<u16>)> = None;
    let mut prev_pc_sig: Option<(u32, u32, u16, &'static str)> = None;
    let mut prev_intreq = amiga.paula.intreq;
    let mut prev_intena = amiga.paula.intena;
    let mut stop_tick = None;

    for tick in 0..MAX_BOOT_TICKS {
        amiga.tick();

        if stop_tick.is_none()
            && amiga.cpu.regs.pc == spec.stop_resume_pc
            && amiga.cpu.ir == 0x4E72
            && (amiga.paula.intreq & 0x0040) != 0
        {
            let sample = sample_cpu(&amiga, tick);
            report.stop_tick = Some(tick);
            report.stop_state = Some(sample);
            report.pc_samples.push(sample);
            prev_pc_sig = Some((sample.pc, sample.instr_start_pc, sample.ir, sample.state));
            stop_tick = Some(tick);
        }

        let trace_active = stop_tick.is_some();
        if !trace_active {
            continue;
        }

        if amiga.paula.intreq != prev_intreq {
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
        if amiga.paula.intena != prev_intena {
            report.intena_changes.push(RegisterChange {
                tick,
                old_value: prev_intena,
                new_value: amiga.paula.intena,
                pc: amiga.cpu.regs.pc,
                instr_start_pc: amiga.cpu.instr_start_pc,
                ir: amiga.cpu.ir,
                sr: amiga.cpu.regs.sr,
            });
            prev_intena = amiga.paula.intena;
        }

        let current_bus_sig = match &amiga.cpu.state {
            State::BusCycle {
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

                if fc == FunctionCode::InterruptAck && report.iack_events.len() < MAX_IACK_EVENTS {
                    report.iack_events.push(IackEvent {
                        tick,
                        level: amiga.paula.compute_ipl(),
                        pc: amiga.cpu.regs.pc,
                        instr_start_pc: amiga.cpu.instr_start_pc,
                        ir: amiga.cpu.ir,
                        sr: amiga.cpu.regs.sr,
                        intena: amiga.paula.intena,
                        intreq: amiga.paula.intreq,
                    });
                } else if !is_read && report.custom_write_events.len() < MAX_CUSTOM_WRITES {
                    let addr24 = addr & 0x00FF_FFFF;
                    if (addr24 & 0xFFF000) == 0xDFF000 {
                        let offset = (addr24 & 0x01FE) as u16;
                        if let Some(register) = custom_register_name(offset) {
                            let raw_data = data.unwrap_or(0);
                            let effective_data = if is_word {
                                raw_data
                            } else if addr24 & 1 == 0 {
                                raw_data << 8
                            } else {
                                raw_data & 0x00FF
                            };
                            report.custom_write_events.push(CustomWriteEvent {
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

        if tick % 4 == 0 && report.pc_samples.len() < MAX_PC_SAMPLES {
            let state = state_name(&amiga.cpu.state);
            let sig = (amiga.cpu.regs.pc, amiga.cpu.instr_start_pc, amiga.cpu.ir, state);
            if Some(sig) != prev_pc_sig {
                report.pc_samples.push(sample_cpu(&amiga, tick));
                prev_pc_sig = Some(sig);
            }
        }

        let elapsed_since_stop = tick - stop_tick.expect("trace active implies stop tick");
        if elapsed_since_stop >= POST_STOP_TRACE_TICKS
            || (report.iack_events.len() >= MAX_IACK_EVENTS
                && report.pc_samples.len() >= 32
                && report.custom_write_events.len() >= 4)
        {
            break;
        }
    }

    assert!(report.stop_tick.is_some(), "did not reach STOP loop");
    assert!(
        !report.iack_events.is_empty(),
        "did not observe any interrupt acknowledge cycles after STOP"
    );

    write_report(spec.report_name, &report);
}

#[test]
#[ignore]
fn trace_first_l3_irq_a3000() {
    run_irq3_trace(&TraceSpec {
        model: AmigaModel::A3000,
        chipset: AmigaChipset::Ecs,
        rom_path: "../../roms/kick31_40_068_a3000.rom",
        report_name: "irq3_trace_kick31_a3000",
        stop_resume_pc: 0x00F81496,
    });
}

#[test]
#[ignore]
fn trace_first_l3_irq_a4000() {
    run_irq3_trace(&TraceSpec {
        model: AmigaModel::A4000,
        chipset: AmigaChipset::Aga,
        rom_path: "../../roms/kick31_40_068_a4000.rom",
        report_name: "irq3_trace_kick31_a4000",
        stop_resume_pc: 0x00F8147E,
    });
}
