//! Focused post-SDMAC-pending IRQ6 trace for KS3.1 A3000.
//!
//! This test is ignored by default because it requires a local Kickstart ROM
//! and writes JSON under `test_output/amiga/traces/`.

mod common;

use std::fs;
use std::path::PathBuf;

use machine_amiga::{Amiga, AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};
use motorola_68000::bus::FunctionCode;
use motorola_68000::cpu::State;
use serde::Serialize;

use common::load_rom;

const MAX_BOOT_TICKS: u64 = 130_000_000;
const POST_PENDING_TRACE_TICKS: u64 = 200_000;
const MAX_IACK_EVENTS: usize = 16;
const MAX_DMAC_CHANGES: usize = 32;
const MAX_BOARD_IO_EVENTS: usize = 64;
const MAX_TASK_EVENTS: usize = 16;
const MAX_PC_SAMPLES: usize = 96;

#[derive(Serialize)]
struct TraceReport {
    rom_path: &'static str,
    pending_tick: u64,
    pending_state: CpuSample,
    pending_dmac: DmacSnapshot,
    stop_tick: Option<u64>,
    iack_events: Vec<IackEvent>,
    dmac_changes: Vec<DmacChange>,
    board_io_events: Vec<BoardIoEvent>,
    task_events: Vec<TaskEvent>,
    pc_samples: Vec<CpuSample>,
}

#[derive(Clone, Copy, Serialize)]
struct CpuSample {
    tick: u64,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    sr: u16,
    intena: u16,
    intreq: u16,
}

#[derive(Clone, Copy, Serialize, PartialEq, Eq)]
struct DmacSnapshot {
    cntr: u8,
    istr: u8,
    dawr: u8,
    wtc: u32,
    acr: u32,
    wd_selected_reg: u8,
    wd_asr: u8,
    wd_scsi_status: u8,
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
    dmac: DmacSnapshot,
}

#[derive(Serialize)]
struct DmacChange {
    tick: u64,
    old: DmacSnapshot,
    new: DmacSnapshot,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    sr: u16,
    intena: u16,
    intreq: u16,
}

#[derive(Serialize)]
struct BoardIoEvent {
    tick: u64,
    addr: u32,
    is_read: bool,
    size: &'static str,
    raw_data: Option<u16>,
    effective_data: Option<u32>,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    sr: u16,
    intena: u16,
    intreq: u16,
    dmac: DmacSnapshot,
}

#[derive(Serialize)]
struct TaskEvent {
    tick: u64,
    field: &'static str,
    old_value: u32,
    new_value: u32,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    sr: u16,
    stack_return_pc: u32,
    intena: u16,
    intreq: u16,
    dmac: DmacSnapshot,
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
    let data = serde_json::to_vec_pretty(report).expect("serialize IRQ6 trace report");
    fs::write(&path, data).expect("write IRQ6 trace report");
    println!("IRQ6 trace saved to {}", path.display());
}

fn build_amiga() -> Option<Amiga> {
    let rom = load_rom("../../roms/kick31_40_068_a3000.rom")?;
    Some(Amiga::new_with_config(AmigaConfig {
        model: AmigaModel::A3000,
        chipset: AmigaChipset::Ecs,
        region: AmigaRegion::Pal,
        kickstart: rom,
        slow_ram_size: 0,
    }))
}

fn read_bus_byte(amiga: &Amiga, addr: u32) -> u8 {
    if !amiga.memory.fast_ram.is_empty() {
        let base = amiga.memory.fast_ram_base;
        let end = base.wrapping_add(amiga.memory.fast_ram.len() as u32);
        if addr >= base && addr < end {
            let offset = (addr - base) & amiga.memory.fast_ram_mask;
            return amiga.memory.fast_ram[offset as usize];
        }
    }
    amiga.memory.read_byte(addr)
}

fn read_bus_word(amiga: &Amiga, addr: u32) -> u16 {
    (u16::from(read_bus_byte(amiga, addr)) << 8) | u16::from(read_bus_byte(amiga, addr + 1))
}

fn read_bus_long(amiga: &Amiga, addr: u32) -> u32 {
    (u32::from(read_bus_word(amiga, addr)) << 16) | u32::from(read_bus_word(amiga, addr + 2))
}

fn sample_cpu(amiga: &Amiga, tick: u64) -> CpuSample {
    CpuSample {
        tick,
        pc: amiga.cpu.regs.pc,
        instr_start_pc: amiga.cpu.instr_start_pc,
        ir: amiga.cpu.ir,
        sr: amiga.cpu.regs.sr,
        intena: amiga.paula.intena,
        intreq: amiga.paula.intreq,
    }
}

fn sample_dmac(amiga: &Amiga) -> DmacSnapshot {
    let dmac = amiga.dmac.as_ref().expect("A3000 should expose SDMAC");
    DmacSnapshot {
        cntr: dmac.cntr(),
        istr: dmac.current_istr(),
        dawr: dmac.dawr(),
        wtc: dmac.wtc(),
        acr: dmac.acr(),
        wd_selected_reg: dmac.wd_selected_reg(),
        wd_asr: dmac.wd_asr(),
        wd_scsi_status: dmac.wd_scsi_status(),
    }
}

fn run_irq6_trace() {
    let Some(mut amiga) = build_amiga() else {
        return;
    };

    let mut pending_tick = None;
    let mut pending_state = None;
    let mut pending_dmac = None;
    let mut previous_dmac = sample_dmac(&amiga);
    let mut prev_bus_sig: Option<(u32, u8, bool, bool, Option<u16>)> = None;
    let mut prev_pc_sig = None;
    let mut prev_task_state = u32::from(read_bus_byte(&amiga, 132_133_496 + 0x0F));
    let mut prev_task_sig_recvd = read_bus_long(&amiga, 132_133_496 + 0x1A);
    let stop_resume_pc = 0x00F81496;

    let mut report = None;

    for tick in 0..MAX_BOOT_TICKS {
        amiga.tick();

        if pending_tick.is_none() && sample_dmac(&amiga).istr & 0x10 != 0 {
            pending_tick = Some(tick);
            pending_state = Some(sample_cpu(&amiga, tick));
            pending_dmac = Some(sample_dmac(&amiga));
            report = Some(TraceReport {
                rom_path: "../../roms/kick31_40_068_a3000.rom",
                pending_tick: tick,
                pending_state: sample_cpu(&amiga, tick),
                pending_dmac: sample_dmac(&amiga),
                stop_tick: None,
                iack_events: Vec::new(),
                dmac_changes: Vec::new(),
                board_io_events: Vec::new(),
                task_events: Vec::new(),
                pc_samples: vec![sample_cpu(&amiga, tick)],
            });
            prev_pc_sig = Some((amiga.cpu.regs.pc, amiga.cpu.instr_start_pc, amiga.cpu.ir));
        }

        let Some(pending_tick) = pending_tick else {
            continue;
        };
        let report = report.as_mut().expect("report starts with pending tick");

        if report.stop_tick.is_none() && amiga.cpu.regs.pc == stop_resume_pc && amiga.cpu.ir == 0x4E72
        {
            report.stop_tick = Some(tick);
        }

        let current_dmac = sample_dmac(&amiga);
        if current_dmac != previous_dmac && report.dmac_changes.len() < MAX_DMAC_CHANGES {
            report.dmac_changes.push(DmacChange {
                tick,
                old: previous_dmac,
                new: current_dmac,
                pc: amiga.cpu.regs.pc,
                instr_start_pc: amiga.cpu.instr_start_pc,
                ir: amiga.cpu.ir,
                sr: amiga.cpu.regs.sr,
                intena: amiga.paula.intena,
                intreq: amiga.paula.intreq,
            });
            previous_dmac = current_dmac;
        }

        let current_task_state = u32::from(read_bus_byte(&amiga, 132_133_496 + 0x0F));
        if current_task_state != prev_task_state && report.task_events.len() < MAX_TASK_EVENTS {
            report.task_events.push(TaskEvent {
                tick,
                field: "task.tc_state",
                old_value: prev_task_state,
                new_value: current_task_state,
                pc: amiga.cpu.regs.pc,
                instr_start_pc: amiga.cpu.instr_start_pc,
                ir: amiga.cpu.ir,
                sr: amiga.cpu.regs.sr,
                stack_return_pc: read_bus_long(&amiga, amiga.cpu.regs.a(7)),
                intena: amiga.paula.intena,
                intreq: amiga.paula.intreq,
                dmac: current_dmac,
            });
            prev_task_state = current_task_state;
        }

        let current_task_sig_recvd = read_bus_long(&amiga, 132_133_496 + 0x1A);
        if current_task_sig_recvd != prev_task_sig_recvd && report.task_events.len() < MAX_TASK_EVENTS
        {
            report.task_events.push(TaskEvent {
                tick,
                field: "task.tc_sig_recvd",
                old_value: prev_task_sig_recvd,
                new_value: current_task_sig_recvd,
                pc: amiga.cpu.regs.pc,
                instr_start_pc: amiga.cpu.instr_start_pc,
                ir: amiga.cpu.ir,
                sr: amiga.cpu.regs.sr,
                stack_return_pc: read_bus_long(&amiga, amiga.cpu.regs.a(7)),
                intena: amiga.paula.intena,
                intreq: amiga.paula.intreq,
                dmac: current_dmac,
            });
            prev_task_sig_recvd = current_task_sig_recvd;
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
                        dmac: current_dmac,
                    });
                } else if (0xDD_0000..0xDE_0000).contains(&addr)
                    && report.board_io_events.len() < MAX_BOARD_IO_EVENTS
                {
                    let effective_data = if is_read {
                        None
                    } else {
                        let raw_data = data.unwrap_or(0);
                        Some(if is_word {
                            u32::from(raw_data)
                        } else if addr & 1 == 0 {
                            u32::from(raw_data >> 8)
                        } else {
                            u32::from(raw_data & 0x00FF)
                        })
                    };
                    report.board_io_events.push(BoardIoEvent {
                        tick,
                        addr,
                        is_read,
                        size: if is_word { "word" } else { "byte" },
                        raw_data: if is_read { None } else { data },
                        effective_data,
                        pc: amiga.cpu.regs.pc,
                        instr_start_pc: amiga.cpu.instr_start_pc,
                        ir: amiga.cpu.ir,
                        sr: amiga.cpu.regs.sr,
                        intena: amiga.paula.intena,
                        intreq: amiga.paula.intreq,
                        dmac: current_dmac,
                    });
                }
            }
        }
        prev_bus_sig = current_bus_sig;

        if tick % 4 == 0 && report.pc_samples.len() < MAX_PC_SAMPLES {
            let sig = (amiga.cpu.regs.pc, amiga.cpu.instr_start_pc, amiga.cpu.ir);
            if Some(sig) != prev_pc_sig {
                report.pc_samples.push(sample_cpu(&amiga, tick));
                prev_pc_sig = Some(sig);
            }
        }

        if tick - pending_tick >= POST_PENDING_TRACE_TICKS {
            break;
        }
    }

    let report = report.expect("should observe SDMAC pending interrupt");
    write_report("irq6_trace_kick31_a3000", &report);
}

#[test]
#[ignore]
fn trace_first_l6_irq_a3000() {
    run_irq6_trace();
}
