//! Focused storage wait trace for the KS3.1 A3000/A4000 boot stalls.
//!
//! These tests are ignored by default because they require local Kickstart ROMs
//! and write JSON reports under `test_output/amiga/traces/`.

mod common;

use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;

use machine_amiga::{Amiga, AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};
use motorola_68000::bus::FunctionCode;
use motorola_68000::cpu::State;
use serde::Serialize;

use common::load_rom;

const MAX_BOOT_TICKS: u64 = 130_000_000;
const PRE_STOP_WINDOW_TICKS: u64 = 4_000_000;
const MAX_BOARD_IO_EVENTS: usize = 256;
const MAX_TASK_EVENTS: usize = 64;
const MAX_DMAC_CHANGES: usize = 64;
const MAX_GAYLE_CHANGES: usize = 64;
const MAX_TASK_NAME_LEN: usize = 64;

struct TraceSpec {
    model: AmigaModel,
    chipset: AmigaChipset,
    rom_path: &'static str,
    report_name: &'static str,
    stop_resume_pc: u32,
}

#[derive(Clone, Copy)]
struct StopContext {
    stop_tick: u64,
    current_task: u32,
}

#[derive(Serialize)]
struct StorageWaitTraceReport {
    model: &'static str,
    rom_path: &'static str,
    stop_resume_pc: u32,
    stop_tick: u64,
    window_start_tick: u64,
    current_task: u32,
    current_task_name: Option<String>,
    board_io_events: Vec<BoardIoEvent>,
    task_events: Vec<TaskEvent>,
    dmac_changes: Vec<DmacChange>,
    gayle_changes: Vec<GayleChange>,
    final_dmac: Option<DmacSnapshot>,
    final_gayle: Option<GayleSnapshot>,
}

#[derive(Serialize)]
struct BoardIoEvent {
    tick: u64,
    component: &'static str,
    addr: u32,
    fc: &'static str,
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
    dmac: Option<DmacSnapshot>,
    gayle: Option<GayleSnapshot>,
}

#[derive(Clone, Copy, Serialize)]
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
    dmac: Option<DmacSnapshot>,
    gayle: Option<GayleSnapshot>,
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

#[derive(Clone, Copy, Serialize)]
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

#[derive(Clone, Copy, Serialize, PartialEq, Eq)]
struct GayleSnapshot {
    cs: u8,
    irq: u8,
    int_enable: u8,
    cfg: u8,
    ide_status: u8,
    drive_present: bool,
    ide_irq_pending: bool,
}

#[derive(Clone, Copy, Serialize)]
struct GayleChange {
    tick: u64,
    old: GayleSnapshot,
    new: GayleSnapshot,
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

fn write_report(report_name: &str, report: &StorageWaitTraceReport) {
    let path = report_path(report_name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create trace output directory");
    }
    let data = serde_json::to_vec_pretty(report).expect("serialize storage wait trace report");
    fs::write(&path, data).expect("write storage wait trace report");
    println!("storage wait trace saved to {}", path.display());
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

fn discover_stop_context(spec: &TraceSpec) -> StopContext {
    let mut amiga = build_amiga(spec).expect("load Kickstart ROM for stop discovery");

    for tick in 0..MAX_BOOT_TICKS {
        amiga.tick();
        if amiga.cpu.regs.pc == spec.stop_resume_pc && amiga.cpu.ir == 0x4E72 {
            return StopContext {
                stop_tick: tick,
                current_task: amiga.cpu.regs.a[1],
            };
        }
    }

    panic!("did not reach STOP loop while discovering storage wait context");
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

fn read_c_string(amiga: &Amiga, addr: u32, max_len: usize) -> Option<String> {
    if addr == 0 {
        return None;
    }

    let mut bytes = Vec::with_capacity(max_len.min(32));
    for offset in 0..max_len {
        let byte = read_bus_byte(amiga, addr.wrapping_add(offset as u32));
        if byte == 0 {
            break;
        }
        if !(0x20..=0x7E).contains(&byte) {
            return None;
        }
        bytes.push(byte);
    }

    if bytes.is_empty() {
        None
    } else {
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }
}

fn sample_dmac(amiga: &Amiga) -> Option<DmacSnapshot> {
    let dmac = amiga.dmac.as_ref()?;
    Some(DmacSnapshot {
        cntr: dmac.cntr(),
        istr: dmac.current_istr(),
        dawr: dmac.dawr(),
        wtc: dmac.wtc(),
        acr: dmac.acr(),
        wd_selected_reg: dmac.wd_selected_reg(),
        wd_asr: dmac.wd_asr(),
        wd_scsi_status: dmac.wd_scsi_status(),
    })
}

fn sample_gayle(amiga: &Amiga) -> Option<GayleSnapshot> {
    let gayle = amiga.gayle.as_ref()?;
    Some(GayleSnapshot {
        cs: gayle.cs(),
        irq: gayle.irq(),
        int_enable: gayle.int_enable(),
        cfg: gayle.cfg(),
        ide_status: gayle.ide_status(),
        drive_present: gayle.drive_present(),
        ide_irq_pending: gayle.ide_irq_pending(),
    })
}

fn fc_name(fc: FunctionCode) -> &'static str {
    match fc {
        FunctionCode::InterruptAck => "iack",
        FunctionCode::SupervisorProgram => "supervisor_program",
        FunctionCode::SupervisorData => "supervisor_data",
        FunctionCode::UserProgram => "user_program",
        FunctionCode::UserData => "user_data",
    }
}

fn classify_board_io(addr: u32) -> Option<&'static str> {
    if !(0xD8_0000..0xDF_0000).contains(&addr) {
        return None;
    }

    if (0xDD_0000..0xDE_0000).contains(&addr) {
        Some("dmac")
    } else if (0xDE_0000..0xDF_0000).contains(&addr) {
        Some("resource")
    } else {
        Some("board_io")
    }
}

fn push_ring<T>(items: &mut VecDeque<T>, value: T, max_len: usize) {
    if items.len() == max_len {
        items.pop_front();
    }
    items.push_back(value);
}

fn run_storage_wait_trace(spec: &TraceSpec) {
    let context = discover_stop_context(spec);
    let window_start_tick = context.stop_tick.saturating_sub(PRE_STOP_WINDOW_TICKS);
    let mut amiga = build_amiga(spec).expect("load Kickstart ROM for storage wait trace");

    let task_name_ptr = read_bus_long(&amiga, context.current_task + 0x0A);
    let mut report = StorageWaitTraceReport {
        model: model_name(spec.model),
        rom_path: spec.rom_path,
        stop_resume_pc: spec.stop_resume_pc,
        stop_tick: context.stop_tick,
        window_start_tick,
        current_task: context.current_task,
        current_task_name: read_c_string(&amiga, task_name_ptr, MAX_TASK_NAME_LEN),
        board_io_events: Vec::new(),
        task_events: Vec::new(),
        dmac_changes: Vec::new(),
        gayle_changes: Vec::new(),
        final_dmac: None,
        final_gayle: None,
    };

    let mut board_io_events = VecDeque::with_capacity(MAX_BOARD_IO_EVENTS);
    let mut prev_bus_sig: Option<(u32, u8, bool, bool, Option<u16>)> = None;
    let mut previous_task_state = u32::from(read_bus_byte(&amiga, context.current_task + 0x0F));
    let mut previous_task_sig_wait = read_bus_long(&amiga, context.current_task + 0x16);
    let mut previous_task_sig_recvd = read_bus_long(&amiga, context.current_task + 0x1A);
    let mut previous_dmac = sample_dmac(&amiga);
    let mut previous_gayle = sample_gayle(&amiga);

    for tick in 0..=context.stop_tick {
        amiga.tick();

        if tick < window_start_tick {
            continue;
        }

        let current_task_state = u32::from(read_bus_byte(&amiga, context.current_task + 0x0F));
        if current_task_state != previous_task_state && report.task_events.len() < MAX_TASK_EVENTS {
            report.task_events.push(TaskEvent {
                tick,
                field: "task.tc_state",
                old_value: previous_task_state,
                new_value: current_task_state,
                pc: amiga.cpu.regs.pc,
                instr_start_pc: amiga.cpu.instr_start_pc,
                ir: amiga.cpu.ir,
                sr: amiga.cpu.regs.sr,
                stack_return_pc: read_bus_long(&amiga, amiga.cpu.regs.a(7)),
                intena: amiga.paula.intena,
                intreq: amiga.paula.intreq,
                dmac: sample_dmac(&amiga),
                gayle: sample_gayle(&amiga),
            });
            previous_task_state = current_task_state;
        }

        let current_task_sig_wait = read_bus_long(&amiga, context.current_task + 0x16);
        if current_task_sig_wait != previous_task_sig_wait
            && report.task_events.len() < MAX_TASK_EVENTS
        {
            report.task_events.push(TaskEvent {
                tick,
                field: "task.tc_sig_wait",
                old_value: previous_task_sig_wait,
                new_value: current_task_sig_wait,
                pc: amiga.cpu.regs.pc,
                instr_start_pc: amiga.cpu.instr_start_pc,
                ir: amiga.cpu.ir,
                sr: amiga.cpu.regs.sr,
                stack_return_pc: read_bus_long(&amiga, amiga.cpu.regs.a(7)),
                intena: amiga.paula.intena,
                intreq: amiga.paula.intreq,
                dmac: sample_dmac(&amiga),
                gayle: sample_gayle(&amiga),
            });
            previous_task_sig_wait = current_task_sig_wait;
        }

        let current_task_sig_recvd = read_bus_long(&amiga, context.current_task + 0x1A);
        if current_task_sig_recvd != previous_task_sig_recvd
            && report.task_events.len() < MAX_TASK_EVENTS
        {
            report.task_events.push(TaskEvent {
                tick,
                field: "task.tc_sig_recvd",
                old_value: previous_task_sig_recvd,
                new_value: current_task_sig_recvd,
                pc: amiga.cpu.regs.pc,
                instr_start_pc: amiga.cpu.instr_start_pc,
                ir: amiga.cpu.ir,
                sr: amiga.cpu.regs.sr,
                stack_return_pc: read_bus_long(&amiga, amiga.cpu.regs.a(7)),
                intena: amiga.paula.intena,
                intreq: amiga.paula.intreq,
                dmac: sample_dmac(&amiga),
                gayle: sample_gayle(&amiga),
            });
            previous_task_sig_recvd = current_task_sig_recvd;
        }

        let current_dmac = sample_dmac(&amiga);
        if let (Some(old), Some(new)) = (previous_dmac, current_dmac)
            && old != new
            && report.dmac_changes.len() < MAX_DMAC_CHANGES
        {
            report.dmac_changes.push(DmacChange {
                tick,
                old,
                new,
                pc: amiga.cpu.regs.pc,
                instr_start_pc: amiga.cpu.instr_start_pc,
                ir: amiga.cpu.ir,
                sr: amiga.cpu.regs.sr,
                intena: amiga.paula.intena,
                intreq: amiga.paula.intreq,
            });
        }
        previous_dmac = current_dmac;

        let current_gayle = sample_gayle(&amiga);
        if let (Some(old), Some(new)) = (previous_gayle, current_gayle)
            && old != new
            && report.gayle_changes.len() < MAX_GAYLE_CHANGES
        {
            report.gayle_changes.push(GayleChange {
                tick,
                old,
                new,
                pc: amiga.cpu.regs.pc,
                instr_start_pc: amiga.cpu.instr_start_pc,
                ir: amiga.cpu.ir,
                sr: amiga.cpu.regs.sr,
                intena: amiga.paula.intena,
                intreq: amiga.paula.intreq,
            });
        }
        previous_gayle = current_gayle;

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
            if let Some((addr, fc_bits, is_read, is_word, data)) = current_bus_sig
                && let Some(component) = classify_board_io(addr)
            {
                let fc = match fc_bits {
                    7 => FunctionCode::InterruptAck,
                    6 => FunctionCode::SupervisorProgram,
                    5 => FunctionCode::SupervisorData,
                    2 => FunctionCode::UserProgram,
                    1 => FunctionCode::UserData,
                    _ => unreachable!("invalid function code"),
                };
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

                push_ring(
                    &mut board_io_events,
                    BoardIoEvent {
                        tick,
                        component,
                        addr,
                        fc: fc_name(fc),
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
                        dmac: sample_dmac(&amiga),
                        gayle: sample_gayle(&amiga),
                    },
                    MAX_BOARD_IO_EVENTS,
                );
            }
        }
        prev_bus_sig = current_bus_sig;
    }

    report.board_io_events = board_io_events.into_iter().collect();
    report.final_dmac = sample_dmac(&amiga);
    report.final_gayle = sample_gayle(&amiga);
    write_report(spec.report_name, &report);
}

#[test]
#[ignore]
fn trace_storage_wait_a3000() {
    run_storage_wait_trace(&TraceSpec {
        model: AmigaModel::A3000,
        chipset: AmigaChipset::Ecs,
        rom_path: "../../roms/kick31_40_068_a3000.rom",
        report_name: "storage_wait_trace_kick31_a3000",
        stop_resume_pc: 0x00F81496,
    });
}

#[test]
#[ignore]
fn trace_storage_wait_a4000() {
    run_storage_wait_trace(&TraceSpec {
        model: AmigaModel::A4000,
        chipset: AmigaChipset::Aga,
        rom_path: "../../roms/kick31_40_068_a4000.rom",
        report_name: "storage_wait_trace_kick31_a4000",
        stop_resume_pc: 0x00F8147E,
    });
}
