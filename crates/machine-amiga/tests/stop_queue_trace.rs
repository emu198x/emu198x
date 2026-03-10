//! Focused trace for the KS3.1 STOP-loop queue state on A3000/A4000.
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
use motorola_68000::cpu::State;
use serde::Serialize;

use common::load_rom;

const MAX_BOOT_TICKS: u64 = 120_000_000;
const POST_STOP_TRACE_TICKS: u64 = 4_000_000;
const MAX_IACK_EVENTS: usize = 8;
const MAX_PC_SAMPLES: usize = 96;
const MAX_BLOCK_WRITES: usize = 64;
const MAX_EXEC_CHANGES: usize = 128;
const MAX_TASK_CHANGES: usize = 64;
const MAX_TASK_NAME_LEN: usize = 64;
const MAX_CUSTOM_WRITE_EVENTS: usize = 96;

struct TraceSpec {
    model: AmigaModel,
    chipset: AmigaChipset,
    rom_path: &'static str,
    report_name: &'static str,
    stop_resume_pc: u32,
}

#[derive(Clone, Copy)]
enum WatchSize {
    Byte,
    Word,
    Long,
}

struct WatchField {
    name: &'static str,
    addr: u32,
    size: WatchSize,
}

#[derive(Serialize)]
struct TraceReport {
    model: &'static str,
    rom_path: &'static str,
    stop_resume_pc: u32,
    stop_tick: Option<u64>,
    stop_state: Option<StopState>,
    cia_a_at_stop: Option<CiaSample>,
    watch_fields_at_stop: Vec<WatchedValue>,
    current_task_name: Option<String>,
    current_task_fields_at_stop: Vec<WatchedValue>,
    cia_a_changes: Vec<CiaChange>,
    exec_watched_changes: Vec<WatchedChange>,
    current_task_changes: Vec<WatchedChange>,
    exec_block_write_events: Vec<BlockWriteEvent>,
    current_task_write_events: Vec<BlockWriteEvent>,
    iack_events: Vec<IackEvent>,
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

#[derive(Clone, Copy, Serialize)]
struct StopState {
    tick: u64,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    sr: u16,
    d0: u32,
    d1: u32,
    d2: u32,
    d3: u32,
    a0: u32,
    a1: u32,
    a2: u32,
    a3: u32,
    a4: u32,
    a5: u32,
    a6: u32,
    a7: u32,
    intena: u16,
    intreq: u16,
}

#[derive(Clone, Copy, Serialize)]
struct WatchedValue {
    name: &'static str,
    addr: u32,
    size: &'static str,
    value: u32,
}

#[derive(Serialize)]
struct WatchedChange {
    tick: u64,
    name: &'static str,
    addr: u32,
    size: &'static str,
    old_value: u32,
    new_value: u32,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    sr: u16,
    a7: u32,
    stack_return_pc: u32,
    intena: u16,
    intreq: u16,
    current_entry: Option<TaskSummary>,
}

#[derive(Serialize)]
struct BlockWriteEvent {
    tick: u64,
    addr: u32,
    size: &'static str,
    raw_data: u16,
    effective_data: u32,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    sr: u16,
    intena: u16,
    intreq: u16,
}

#[derive(Clone, Copy, Serialize)]
struct CiaSample {
    timer_a: u16,
    timer_b: u16,
    icr_status: u8,
    icr_mask: u8,
    cra: u8,
    crb: u8,
}

#[derive(Serialize)]
struct CiaChange {
    tick: u64,
    old: CiaSample,
    new: CiaSample,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    sr: u16,
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
    cia_a: CiaSample,
}

#[derive(Serialize)]
struct TaskSummary {
    addr: u32,
    name: Option<String>,
    state: u8,
    sig_wait: u32,
    sig_recvd: u32,
}

#[derive(Clone, Copy)]
struct StopContext {
    stop_tick: u64,
    exec_base: u32,
    current_task: u32,
}

#[derive(Serialize)]
struct ExecWaitTransitionReport {
    model: &'static str,
    rom_path: &'static str,
    stop_resume_pc: u32,
    stop_tick: u64,
    exec_base: u32,
    current_task: u32,
    current_task_name: Option<String>,
    watched_fields_initial: Vec<WatchedValue>,
    watched_changes: Vec<WatchedChange>,
    custom_write_events: Vec<CustomWriteEvent>,
    first_blitter_irq_assert: Option<BlitterIrqDebugEvent>,
}

#[derive(Serialize)]
struct CustomWriteEvent {
    tick: u64,
    reg: &'static str,
    addr: u32,
    size: &'static str,
    raw_data: u16,
    effective_data: u32,
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

fn write_report<T: Serialize>(report_name: &str, report: &T) {
    let path = report_path(report_name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create trace output directory");
    }
    let data = serde_json::to_vec_pretty(report).expect("serialize stop queue trace report");
    fs::write(&path, data).expect("write stop queue trace report");
    println!("stop queue trace saved to {}", path.display());
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
    let mut amiga = build_amiga(spec).expect("load Kickstart ROM for stop context");

    for tick in 0..MAX_BOOT_TICKS {
        amiga.tick();
        if amiga.cpu.regs.pc == spec.stop_resume_pc && amiga.cpu.ir == 0x4E72 {
            return StopContext {
                stop_tick: tick,
                exec_base: amiga.cpu.regs.a[6],
                current_task: amiga.cpu.regs.a[1],
            };
        }
    }

    panic!("did not reach STOP loop while discovering stop context");
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

fn sample_stop_state(amiga: &Amiga, tick: u64) -> StopState {
    StopState {
        tick,
        pc: amiga.cpu.regs.pc,
        instr_start_pc: amiga.cpu.instr_start_pc,
        ir: amiga.cpu.ir,
        sr: amiga.cpu.regs.sr,
        d0: amiga.cpu.regs.d[0],
        d1: amiga.cpu.regs.d[1],
        d2: amiga.cpu.regs.d[2],
        d3: amiga.cpu.regs.d[3],
        a0: amiga.cpu.regs.a[0],
        a1: amiga.cpu.regs.a[1],
        a2: amiga.cpu.regs.a[2],
        a3: amiga.cpu.regs.a[3],
        a4: amiga.cpu.regs.a[4],
        a5: amiga.cpu.regs.a[5],
        a6: amiga.cpu.regs.a[6],
        a7: amiga.cpu.regs.a(7),
        intena: amiga.paula.intena,
        intreq: amiga.paula.intreq,
    }
}

fn sample_cia_a(amiga: &Amiga) -> CiaSample {
    CiaSample {
        timer_a: amiga.cia_a.timer_a(),
        timer_b: amiga.cia_a.timer_b(),
        icr_status: amiga.cia_a.icr_status(),
        icr_mask: amiga.cia_a.icr_mask(),
        cra: amiga.cia_a.cra(),
        crb: amiga.cia_a.crb(),
    }
}

fn cia_a_tuple(sample: CiaSample) -> (u16, u16, u8, u8, u8, u8) {
    (
        sample.timer_a,
        sample.timer_b,
        sample.icr_status,
        sample.icr_mask,
        sample.cra,
        sample.crb,
    )
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

fn watch_size_name(size: WatchSize) -> &'static str {
    match size {
        WatchSize::Byte => "byte",
        WatchSize::Word => "word",
        WatchSize::Long => "long",
    }
}

fn read_watch_value(amiga: &Amiga, field: &WatchField) -> u32 {
    match field.size {
        WatchSize::Byte => u32::from(read_bus_byte(amiga, field.addr)),
        WatchSize::Word => u32::from(read_bus_word(amiga, field.addr)),
        WatchSize::Long => read_bus_long(amiga, field.addr),
    }
}

fn snapshot_watch_fields(amiga: &Amiga, fields: &[WatchField]) -> Vec<WatchedValue> {
    fields
        .iter()
        .map(|field| WatchedValue {
            name: field.name,
            addr: field.addr,
            size: watch_size_name(field.size),
            value: read_watch_value(amiga, field),
        })
        .collect()
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
        if !(0x20..=0x7e).contains(&byte) {
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

fn sample_task_summary(amiga: &Amiga, task: u32) -> Option<TaskSummary> {
    if task == 0 {
        return None;
    }

    Some(TaskSummary {
        addr: task,
        name: read_c_string(amiga, read_bus_long(amiga, task + 0x0A), MAX_TASK_NAME_LEN),
        state: read_bus_byte(amiga, task + 0x0F),
        sig_wait: read_bus_long(amiga, task + 0x16),
        sig_recvd: read_bus_long(amiga, task + 0x1A),
    })
}

fn sample_current_entry(amiga: &Amiga, exec_base: u32) -> Option<TaskSummary> {
    sample_task_summary(amiga, read_bus_long(amiga, exec_base + 0x114))
}

fn stop_watch_fields(a6: u32) -> Vec<WatchField> {
    vec![
        WatchField {
            name: "current_entry",
            addr: a6 + 0x114,
            size: WatchSize::Long,
        },
        WatchField {
            name: "wait_count",
            addr: a6 + 0x118,
            size: WatchSize::Long,
        },
        WatchField {
            name: "dispatch_count",
            addr: a6 + 0x11C,
            size: WatchSize::Long,
        },
        WatchField {
            name: "field_120",
            addr: a6 + 0x120,
            size: WatchSize::Word,
        },
        WatchField {
            name: "field_122",
            addr: a6 + 0x122,
            size: WatchSize::Word,
        },
        WatchField {
            name: "flags_124",
            addr: a6 + 0x124,
            size: WatchSize::Byte,
        },
        WatchField {
            name: "field_126",
            addr: a6 + 0x126,
            size: WatchSize::Word,
        },
        WatchField {
            name: "queue_head",
            addr: a6 + 0x196,
            size: WatchSize::Long,
        },
        WatchField {
            name: "queue_tail",
            addr: a6 + 0x19A,
            size: WatchSize::Long,
        },
        WatchField {
            name: "queue_tail_pred",
            addr: a6 + 0x19E,
            size: WatchSize::Long,
        },
    ]
}

fn task_watch_fields(task: u32) -> Vec<WatchField> {
    vec![
        WatchField {
            name: "task.ln_name",
            addr: task + 0x0A,
            size: WatchSize::Long,
        },
        WatchField {
            name: "task.tc_flags",
            addr: task + 0x0E,
            size: WatchSize::Byte,
        },
        WatchField {
            name: "task.tc_state",
            addr: task + 0x0F,
            size: WatchSize::Byte,
        },
        WatchField {
            name: "task.tc_idnest",
            addr: task + 0x10,
            size: WatchSize::Byte,
        },
        WatchField {
            name: "task.tc_tdnest",
            addr: task + 0x11,
            size: WatchSize::Byte,
        },
        WatchField {
            name: "task.tc_sig_alloc",
            addr: task + 0x12,
            size: WatchSize::Long,
        },
        WatchField {
            name: "task.tc_sig_wait",
            addr: task + 0x16,
            size: WatchSize::Long,
        },
        WatchField {
            name: "task.tc_sig_recvd",
            addr: task + 0x1A,
            size: WatchSize::Long,
        },
        WatchField {
            name: "task.tc_sig_except",
            addr: task + 0x1E,
            size: WatchSize::Long,
        },
        WatchField {
            name: "task.tc_trap_alloc",
            addr: task + 0x22,
            size: WatchSize::Word,
        },
        WatchField {
            name: "task.tc_trap_able",
            addr: task + 0x24,
            size: WatchSize::Word,
        },
        WatchField {
            name: "task.tc_except_data",
            addr: task + 0x26,
            size: WatchSize::Long,
        },
        WatchField {
            name: "task.tc_except_code",
            addr: task + 0x2A,
            size: WatchSize::Long,
        },
        WatchField {
            name: "task.tc_trap_data",
            addr: task + 0x2E,
            size: WatchSize::Long,
        },
        WatchField {
            name: "task.tc_trap_code",
            addr: task + 0x32,
            size: WatchSize::Long,
        },
        WatchField {
            name: "task.tc_spreg",
            addr: task + 0x36,
            size: WatchSize::Long,
        },
    ]
}

fn exec_wait_transition_watch_fields(exec_base: u32, current_task: u32) -> Vec<WatchField> {
    vec![
        WatchField {
            name: "current_entry",
            addr: exec_base + 0x114,
            size: WatchSize::Long,
        },
        WatchField {
            name: "task.tc_state",
            addr: current_task + 0x0F,
            size: WatchSize::Byte,
        },
        WatchField {
            name: "task.tc_sig_wait",
            addr: current_task + 0x16,
            size: WatchSize::Long,
        },
        WatchField {
            name: "task.tc_sig_recvd",
            addr: current_task + 0x1A,
            size: WatchSize::Long,
        },
    ]
}

fn custom_reg_name(addr: u32) -> Option<&'static str> {
    match addr & !1 {
        0x00DFF040 => Some("BLTCON0"),
        0x00DFF042 => Some("BLTCON1"),
        0x00DFF050 => Some("BLTAPTH"),
        0x00DFF052 => Some("BLTBPTH"),
        0x00DFF054 => Some("BLTCPTH"),
        0x00DFF056 => Some("BLTDPTH"),
        0x00DFF058 => Some("BLTSIZE"),
        0x00DFF096 => Some("DMACON"),
        0x00DFF09A => Some("INTENA"),
        0x00DFF09C => Some("INTREQ"),
        _ => None,
    }
}

fn run_stop_queue_trace(spec: &TraceSpec) {
    let Some(mut amiga) = build_amiga(spec) else {
        return;
    };

    let mut report = TraceReport {
        model: model_name(spec.model),
        rom_path: spec.rom_path,
        stop_resume_pc: spec.stop_resume_pc,
        stop_tick: None,
        stop_state: None,
        cia_a_at_stop: None,
        watch_fields_at_stop: Vec::new(),
        current_task_name: None,
        current_task_fields_at_stop: Vec::new(),
        cia_a_changes: Vec::new(),
        exec_watched_changes: Vec::new(),
        current_task_changes: Vec::new(),
        exec_block_write_events: Vec::new(),
        current_task_write_events: Vec::new(),
        iack_events: Vec::new(),
        pc_samples: Vec::new(),
    };

    let mut prev_bus_sig: Option<(u32, u8, bool, bool, Option<u16>)> = None;
    let mut prev_pc_sig: Option<(u32, u32, u16)> = None;
    let mut stop_tick = None;
    let mut exec_base = None;
    let mut exec_watch_fields = Vec::new();
    let mut previous_exec_watch_values = Vec::new();
    let mut current_task_watch_fields = Vec::new();
    let mut previous_current_task_watch_values = Vec::new();
    let mut exec_block_start = 0;
    let mut exec_block_end = 0;
    let mut current_task_block_start = 0;
    let mut current_task_block_end = 0;
    let mut prev_cia_a = sample_cia_a(&amiga);

    for tick in 0..MAX_BOOT_TICKS {
        amiga.tick();

        if stop_tick.is_none() && amiga.cpu.regs.pc == spec.stop_resume_pc && amiga.cpu.ir == 0x4E72
        {
            let stop_state = sample_stop_state(&amiga, tick);
            exec_watch_fields = stop_watch_fields(stop_state.a6);
            exec_base = Some(stop_state.a6);
            previous_exec_watch_values = exec_watch_fields
                .iter()
                .map(|field| read_watch_value(&amiga, field))
                .collect();
            exec_block_start = stop_state.a6 + 0x110;
            exec_block_end = stop_state.a6 + 0x1A8;
            current_task_watch_fields = task_watch_fields(stop_state.a1);
            previous_current_task_watch_values = current_task_watch_fields
                .iter()
                .map(|field| read_watch_value(&amiga, field))
                .collect();
            current_task_block_start = stop_state.a1;
            current_task_block_end = stop_state.a1 + 0x5A;

            report.stop_tick = Some(tick);
            report.stop_state = Some(stop_state);
            report.cia_a_at_stop = Some(sample_cia_a(&amiga));
            report.watch_fields_at_stop = snapshot_watch_fields(&amiga, &exec_watch_fields);
            report.current_task_name = read_c_string(
                &amiga,
                read_bus_long(&amiga, stop_state.a1 + 0x0A),
                MAX_TASK_NAME_LEN,
            );
            report.current_task_fields_at_stop =
                snapshot_watch_fields(&amiga, &current_task_watch_fields);

            let sample = sample_cpu(&amiga, tick);
            report.pc_samples.push(sample);
            prev_pc_sig = Some((sample.pc, sample.instr_start_pc, sample.ir));
            stop_tick = Some(tick);
        }

        if stop_tick.is_none() {
            continue;
        }

        for (index, field) in exec_watch_fields.iter().enumerate() {
            if report.exec_watched_changes.len() >= MAX_EXEC_CHANGES {
                break;
            }

            let current = read_watch_value(&amiga, field);
            let previous = previous_exec_watch_values[index];
            if current != previous {
                report.exec_watched_changes.push(WatchedChange {
                    tick,
                    name: field.name,
                    addr: field.addr,
                    size: watch_size_name(field.size),
                    old_value: previous,
                    new_value: current,
                    pc: amiga.cpu.regs.pc,
                    instr_start_pc: amiga.cpu.instr_start_pc,
                    ir: amiga.cpu.ir,
                    sr: amiga.cpu.regs.sr,
                    a7: amiga.cpu.regs.a(7),
                    stack_return_pc: read_bus_long(&amiga, amiga.cpu.regs.a(7)),
                    intena: amiga.paula.intena,
                    intreq: amiga.paula.intreq,
                    current_entry: exec_base.and_then(|base| sample_current_entry(&amiga, base)),
                });
                previous_exec_watch_values[index] = current;
            }
        }

        for (index, field) in current_task_watch_fields.iter().enumerate() {
            if report.current_task_changes.len() >= MAX_TASK_CHANGES {
                break;
            }

            let current = read_watch_value(&amiga, field);
            let previous = previous_current_task_watch_values[index];
            if current != previous {
                report.current_task_changes.push(WatchedChange {
                    tick,
                    name: field.name,
                    addr: field.addr,
                    size: watch_size_name(field.size),
                    old_value: previous,
                    new_value: current,
                    pc: amiga.cpu.regs.pc,
                    instr_start_pc: amiga.cpu.instr_start_pc,
                    ir: amiga.cpu.ir,
                    sr: amiga.cpu.regs.sr,
                    a7: amiga.cpu.regs.a(7),
                    stack_return_pc: read_bus_long(&amiga, amiga.cpu.regs.a(7)),
                    intena: amiga.paula.intena,
                    intreq: amiga.paula.intreq,
                    current_entry: exec_base.and_then(|base| sample_current_entry(&amiga, base)),
                });
                previous_current_task_watch_values[index] = current;
            }
        }

        let cia_a = sample_cia_a(&amiga);
        if cia_a_tuple(cia_a) != cia_a_tuple(prev_cia_a) {
            if report.cia_a_changes.len() < MAX_EXEC_CHANGES {
                report.cia_a_changes.push(CiaChange {
                    tick,
                    old: prev_cia_a,
                    new: cia_a,
                    pc: amiga.cpu.regs.pc,
                    instr_start_pc: amiga.cpu.instr_start_pc,
                    ir: amiga.cpu.ir,
                    sr: amiga.cpu.regs.sr,
                });
            }
            prev_cia_a = cia_a;
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
                        cia_a,
                    });
                } else if !is_read {
                    let raw_data = data.unwrap_or(0);
                    let effective_data = if is_word {
                        u32::from(raw_data)
                    } else if addr & 1 == 0 {
                        u32::from(raw_data >> 8)
                    } else {
                        u32::from(raw_data & 0x00FF)
                    };

                    let event = BlockWriteEvent {
                        tick,
                        addr,
                        size: if is_word { "word" } else { "byte" },
                        raw_data,
                        effective_data,
                        pc: amiga.cpu.regs.pc,
                        instr_start_pc: amiga.cpu.instr_start_pc,
                        ir: amiga.cpu.ir,
                        sr: amiga.cpu.regs.sr,
                        intena: amiga.paula.intena,
                        intreq: amiga.paula.intreq,
                    };

                    if report.exec_block_write_events.len() < MAX_BLOCK_WRITES
                        && addr >= exec_block_start
                        && addr < exec_block_end
                    {
                        report.exec_block_write_events.push(event);
                    } else if report.current_task_write_events.len() < MAX_BLOCK_WRITES
                        && addr >= current_task_block_start
                        && addr < current_task_block_end
                    {
                        report.current_task_write_events.push(event);
                    }
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

        let elapsed_since_stop = tick - stop_tick.expect("trace active implies stop tick");
        if elapsed_since_stop >= POST_STOP_TRACE_TICKS
            || (report.iack_events.len() >= MAX_IACK_EVENTS
                && report.exec_block_write_events.len() >= 16
                && report.exec_watched_changes.len() >= 16)
        {
            break;
        }
    }

    assert!(report.stop_tick.is_some(), "did not reach STOP loop");
    write_report(spec.report_name, &report);
}

fn run_exec_wait_transition_trace(spec: &TraceSpec) {
    let context = discover_stop_context(spec);
    let mut amiga = build_amiga(spec).expect("load Kickstart ROM for exec wait trace");
    let watch_fields = exec_wait_transition_watch_fields(context.exec_base, context.current_task);
    let mut previous_watch_values = watch_fields
        .iter()
        .map(|field| read_watch_value(&amiga, field))
        .collect::<Vec<_>>();
    let mut prev_bus_sig: Option<(u32, bool, bool, Option<u16>)> = None;
    let mut report = ExecWaitTransitionReport {
        model: model_name(spec.model),
        rom_path: spec.rom_path,
        stop_resume_pc: spec.stop_resume_pc,
        stop_tick: context.stop_tick,
        exec_base: context.exec_base,
        current_task: context.current_task,
        current_task_name: None,
        watched_fields_initial: snapshot_watch_fields(&amiga, &watch_fields),
        watched_changes: Vec::new(),
        custom_write_events: Vec::new(),
        first_blitter_irq_assert: None,
    };

    for tick in 0..=context.stop_tick.saturating_add(1) {
        amiga.tick();

        for (index, field) in watch_fields.iter().enumerate() {
            if report.watched_changes.len() >= MAX_EXEC_CHANGES {
                break;
            }

            let current = read_watch_value(&amiga, field);
            let previous = previous_watch_values[index];
            if current != previous {
                report.watched_changes.push(WatchedChange {
                    tick,
                    name: field.name,
                    addr: field.addr,
                    size: watch_size_name(field.size),
                    old_value: previous,
                    new_value: current,
                    pc: amiga.cpu.regs.pc,
                    instr_start_pc: amiga.cpu.instr_start_pc,
                    ir: amiga.cpu.ir,
                    sr: amiga.cpu.regs.sr,
                    a7: amiga.cpu.regs.a(7),
                    stack_return_pc: read_bus_long(&amiga, amiga.cpu.regs.a(7)),
                    intena: amiga.paula.intena,
                    intreq: amiga.paula.intreq,
                    current_entry: sample_current_entry(&amiga, context.exec_base),
                });
                previous_watch_values[index] = current;
            }
        }

        let current_bus_sig = match &amiga.cpu.state {
            State::BusCycle {
                addr,
                is_read,
                is_word,
                data,
                ..
            } => Some((*addr, *is_read, *is_word, *data)),
            _ => None,
        };

        if current_bus_sig != prev_bus_sig {
            if let Some((addr, is_read, is_word, data)) = current_bus_sig {
                if !is_read && report.custom_write_events.len() < MAX_CUSTOM_WRITE_EVENTS {
                    if let Some(reg) = custom_reg_name(addr) {
                        let raw_data = data.unwrap_or(0);
                        let effective_data = if is_word {
                            u32::from(raw_data)
                        } else if addr & 1 == 0 {
                            u32::from(raw_data >> 8)
                        } else {
                            u32::from(raw_data & 0x00FF)
                        };

                        report.custom_write_events.push(CustomWriteEvent {
                            tick,
                            reg,
                            addr,
                            size: if is_word { "word" } else { "byte" },
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
        prev_bus_sig = current_bus_sig;

        if amiga.cpu.regs.pc == spec.stop_resume_pc && amiga.cpu.ir == 0x4E72 {
            assert_eq!(amiga.cpu.regs.a[6], context.exec_base);
            assert_eq!(amiga.cpu.regs.a[1], context.current_task);
            report.current_task_name = read_c_string(
                &amiga,
                read_bus_long(&amiga, context.current_task + 0x0A),
                MAX_TASK_NAME_LEN,
            );
            report.first_blitter_irq_assert = amiga.first_blitter_irq_assert();
            let report_name = format!("exec_wait_transition_{}", model_name(spec.model));
            write_report(&report_name, &report);
            return;
        }
    }

    panic!("did not reach STOP loop during exec wait transition trace");
}

#[test]
#[ignore]
fn trace_stop_queue_a3000() {
    run_stop_queue_trace(&TraceSpec {
        model: AmigaModel::A3000,
        chipset: AmigaChipset::Ecs,
        rom_path: "../../roms/kick31_40_068_a3000.rom",
        report_name: "stop_queue_trace_kick31_a3000",
        stop_resume_pc: 0x00F81496,
    });
}

#[test]
#[ignore]
fn trace_stop_queue_a4000() {
    run_stop_queue_trace(&TraceSpec {
        model: AmigaModel::A4000,
        chipset: AmigaChipset::Aga,
        rom_path: "../../roms/kick31_40_068_a4000.rom",
        report_name: "stop_queue_trace_kick31_a4000",
        stop_resume_pc: 0x00F8147E,
    });
}

#[test]
#[ignore]
fn trace_exec_wait_transition_a3000() {
    run_exec_wait_transition_trace(&TraceSpec {
        model: AmigaModel::A3000,
        chipset: AmigaChipset::Ecs,
        rom_path: "../../roms/kick31_40_068_a3000.rom",
        report_name: "stop_queue_trace_kick31_a3000",
        stop_resume_pc: 0x00F81496,
    });
}

#[test]
#[ignore]
fn trace_exec_wait_transition_a4000() {
    run_exec_wait_transition_trace(&TraceSpec {
        model: AmigaModel::A4000,
        chipset: AmigaChipset::Aga,
        rom_path: "../../roms/kick31_40_068_a4000.rom",
        report_name: "stop_queue_trace_kick31_a4000",
        stop_resume_pc: 0x00F8147E,
    });
}
