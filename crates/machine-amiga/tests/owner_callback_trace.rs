//! Focused trace for the late A3000 owner callback binding and any follow-up
//! wake activity before the machine reaches the final `STOP`.
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
const STOP_RESUME_PC: u32 = 0x00F8_1496;
const WAIT_NODE_HELPER_START: u32 = 0x00FA_4918;
const POST_STOP_TRACE_TICKS: u64 = 1_024;
const MAX_EVENTS: usize = 512;
const TASK_NAME_MAX_LEN: usize = 64;
const TASK_NAME_PTR_OFFSET: u32 = 0x0A;
const TASK_STATE_OFFSET: u32 = 0x0F;
const TASK_SIG_WAIT_OFFSET: u32 = 0x16;
const TASK_SIG_RECVD_OFFSET: u32 = 0x1A;
const OWNER_EXECBASE_PTR_OFFSET: u32 = 0x1A4;
const OWNER_GATE_BITS_OFFSET: u32 = 0x1C0;
const OWNER_CALLBACK_OFFSET: u32 = 0x1D0;
const OWNER_CONTEXT_OFFSET: u32 = 0x1D4;
const PRODUCER_WAKE_FLAG_OFFSET: u32 = 0x0BE2;
const HELPER_GATE_PC: u32 = 0x00FA_5240;
const HELPER_GATE_BIT_PC: u32 = 0x00FA_5244;
const HELPER_CALLBACK_LOAD_PC: u32 = 0x00FA_524C;
const HELPER_GATE_SET_PC: u32 = 0x00FA_5342;
const HELPER_GATE_CLEAR_PC: u32 = 0x00FA_5716;
const CALLBACK_WRAPPER_PC: u32 = 0x00FC_DB92;
const CALLBACK_SET_FLAG_PC: u32 = 0x00FD_F6D8;
const CALLBACK_DISPATCH_PC: u32 = 0x00FD_F6E4;
const PRODUCER_PATH_PC: u32 = 0x00FD_F310;

#[derive(Clone, Copy)]
struct OwnerContext {
    helper_entry_tick: u64,
    owner_addr: u32,
}

#[derive(Clone, Copy)]
struct StopContext {
    stop_tick: u64,
}

#[derive(Clone, Serialize)]
struct TaskSummary {
    addr: u32,
    name: Option<String>,
    state: u8,
    sig_wait: u32,
    sig_recvd: u32,
}

#[derive(Clone, Serialize)]
struct RegisterSnapshot {
    d0: u32,
    d1: u32,
    d2: u32,
    a0: u32,
    a1: u32,
    a2: u32,
    a5: u32,
    a6: u32,
    a7: u32,
}

#[derive(Clone, Copy, Serialize, PartialEq, Eq)]
struct OwnerBindingSig {
    gate_bits: u8,
    callback_ptr: u32,
    context_ptr: u32,
    producer_wake_flag: u32,
}

#[derive(Serialize)]
struct TraceEvent {
    tick: u64,
    kind: &'static str,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    sr: u16,
    owner: OwnerBindingSig,
    registers: RegisterSnapshot,
    stack_chain: [u32; 12],
    current_entry: Option<TaskSummary>,
}

#[derive(Serialize)]
struct TraceReport {
    rom_path: &'static str,
    trace_start_tick: u64,
    helper_entry_tick: u64,
    stop_tick: u64,
    owner_addr: u32,
    final_owner: OwnerBindingSig,
    events: Vec<TraceEvent>,
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
    let data = serde_json::to_vec_pretty(report).expect("serialize owner callback trace report");
    fs::write(&path, data).expect("write owner callback trace report");
    println!("owner callback trace saved to {}", path.display());
}

fn build_amiga() -> Option<Amiga> {
    let rom = load_rom("../../roms/kick31_40_068_a3000.rom")?;
    Some(Amiga::new_with_config(AmigaConfig {
        model: AmigaModel::A3000,
        chipset: AmigaChipset::Ecs,
        region: AmigaRegion::Pal,
        kickstart: rom,
        slow_ram_size: 0,
            ide_disk: None,
            scsi_disk: None,
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

fn is_ram_addr(amiga: &Amiga, addr: u32) -> bool {
    if (addr as usize) < amiga.memory.chip_ram.len() {
        return true;
    }

    if !amiga.memory.fast_ram.is_empty() {
        let base = amiga.memory.fast_ram_base;
        let end = base.wrapping_add(amiga.memory.fast_ram.len() as u32);
        if addr >= base && addr < end {
            return true;
        }
    }

    false
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

fn sample_task(amiga: &Amiga, task: u32) -> Option<TaskSummary> {
    if task == 0 || !is_ram_addr(amiga, task) {
        return None;
    }

    Some(TaskSummary {
        addr: task,
        name: read_c_string(
            amiga,
            read_bus_long(amiga, task + TASK_NAME_PTR_OFFSET),
            TASK_NAME_MAX_LEN,
        ),
        state: read_bus_byte(amiga, task + TASK_STATE_OFFSET),
        sig_wait: read_bus_long(amiga, task + TASK_SIG_WAIT_OFFSET),
        sig_recvd: read_bus_long(amiga, task + TASK_SIG_RECVD_OFFSET),
    })
}

fn sample_current_entry(amiga: &Amiga, owner_addr: u32) -> Option<TaskSummary> {
    let exec_base = read_bus_long(amiga, owner_addr + OWNER_EXECBASE_PTR_OFFSET);
    if !is_ram_addr(amiga, exec_base) {
        return None;
    }

    let current_entry_addr = read_bus_long(amiga, exec_base + 0x114);
    sample_task(amiga, current_entry_addr)
}

fn sample_registers(amiga: &Amiga) -> RegisterSnapshot {
    RegisterSnapshot {
        d0: amiga.cpu.regs.d[0],
        d1: amiga.cpu.regs.d[1],
        d2: amiga.cpu.regs.d[2],
        a0: amiga.cpu.regs.a[0],
        a1: amiga.cpu.regs.a[1],
        a2: amiga.cpu.regs.a[2],
        a5: amiga.cpu.regs.a[5],
        a6: amiga.cpu.regs.a[6],
        a7: amiga.cpu.regs.a(7),
    }
}

fn stack_chain(amiga: &Amiga) -> [u32; 12] {
    let sp = amiga.cpu.regs.a(7);
    [
        read_bus_long(amiga, sp),
        read_bus_long(amiga, sp + 4),
        read_bus_long(amiga, sp + 8),
        read_bus_long(amiga, sp + 12),
        read_bus_long(amiga, sp + 16),
        read_bus_long(amiga, sp + 20),
        read_bus_long(amiga, sp + 24),
        read_bus_long(amiga, sp + 28),
        read_bus_long(amiga, sp + 32),
        read_bus_long(amiga, sp + 36),
        read_bus_long(amiga, sp + 40),
        read_bus_long(amiga, sp + 44),
    ]
}

fn sample_owner_binding(amiga: &Amiga, owner_addr: u32) -> OwnerBindingSig {
    let gate_bits = read_bus_byte(amiga, owner_addr + OWNER_GATE_BITS_OFFSET);
    let callback_ptr = read_bus_long(amiga, owner_addr + OWNER_CALLBACK_OFFSET);
    let context_ptr = read_bus_long(amiga, owner_addr + OWNER_CONTEXT_OFFSET);
    let producer_wake_flag = if is_ram_addr(amiga, context_ptr) {
        read_bus_long(amiga, context_ptr + PRODUCER_WAKE_FLAG_OFFSET)
    } else {
        0
    };
    OwnerBindingSig {
        gate_bits,
        callback_ptr,
        context_ptr,
        producer_wake_flag,
    }
}

fn make_event(amiga: &Amiga, tick: u64, kind: &'static str, owner_addr: u32) -> TraceEvent {
    TraceEvent {
        tick,
        kind,
        pc: amiga.cpu.regs.pc,
        instr_start_pc: amiga.cpu.instr_start_pc,
        ir: amiga.cpu.ir,
        sr: amiga.cpu.regs.sr,
        owner: sample_owner_binding(amiga, owner_addr),
        registers: sample_registers(amiga),
        stack_chain: stack_chain(amiga),
        current_entry: sample_current_entry(amiga, owner_addr),
    }
}

fn discover_owner_context() -> OwnerContext {
    let mut amiga = build_amiga().expect("load Kickstart ROM for owner discovery");

    for tick in 0..MAX_BOOT_TICKS {
        amiga.tick();
        if amiga.cpu.instr_start_pc == WAIT_NODE_HELPER_START {
            let owner_addr = amiga.cpu.regs.a(6);
            assert!(
                is_ram_addr(&amiga, owner_addr),
                "owner callback trace should find a RAM-backed owner slab"
            );
            return OwnerContext {
                helper_entry_tick: tick,
                owner_addr,
            };
        }
    }

    panic!("owner callback trace did not reach the helper");
}

fn discover_stop_context() -> StopContext {
    let mut amiga = build_amiga().expect("load Kickstart ROM for stop discovery");

    for tick in 0..MAX_BOOT_TICKS {
        amiga.tick();
        if amiga.cpu.regs.pc == STOP_RESUME_PC && amiga.cpu.ir == 0x4E72 {
            return StopContext { stop_tick: tick };
        }
    }

    panic!("owner callback trace did not reach the STOP loop");
}

fn run_owner_callback_trace() {
    let owner_context = discover_owner_context();
    let stop_context = discover_stop_context();
    let trace_start_tick = 0;
    let trace_end_tick = stop_context.stop_tick.saturating_add(POST_STOP_TRACE_TICKS);
    let mut amiga = build_amiga().expect("load Kickstart ROM for owner callback trace");
    let mut report = TraceReport {
        rom_path: "../../roms/kick31_40_068_a3000.rom",
        trace_start_tick,
        helper_entry_tick: owner_context.helper_entry_tick,
        stop_tick: stop_context.stop_tick,
        owner_addr: owner_context.owner_addr,
        final_owner: OwnerBindingSig {
            gate_bits: 0,
            callback_ptr: 0,
            context_ptr: 0,
            producer_wake_flag: 0,
        },
        events: Vec::with_capacity(64),
    };
    let mut prev_owner_sig = None;
    let mut prev_bus_sig: Option<(u32, u8, bool, bool, Option<u16>)> = None;
    let mut stop_recorded = false;

    for tick in 0..=trace_end_tick {
        amiga.tick();
        if tick < trace_start_tick {
            continue;
        }

        let owner_sig = sample_owner_binding(&amiga, owner_context.owner_addr);
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

        if report.events.len() < MAX_EVENTS && owner_sig != prev_owner_sig.unwrap_or(owner_sig) {
            report.events.push(make_event(
                &amiga,
                tick,
                "owner_binding_change",
                owner_context.owner_addr,
            ));
        }
        prev_owner_sig = Some(owner_sig);

        if report.events.len() < MAX_EVENTS && current_bus_sig != prev_bus_sig {
            if let Some((addr, fc_bits, is_read, _is_word, _data)) = current_bus_sig {
                let fc = match fc_bits {
                    7 => FunctionCode::InterruptAck,
                    6 => FunctionCode::SupervisorProgram,
                    5 => FunctionCode::SupervisorData,
                    2 => FunctionCode::UserProgram,
                    1 => FunctionCode::UserData,
                    _ => unreachable!("invalid function code"),
                };
                let producer_flag_addr = owner_sig
                    .context_ptr
                    .wrapping_add(PRODUCER_WAKE_FLAG_OFFSET);
                let owner_gate_addr = owner_context.owner_addr + OWNER_GATE_BITS_OFFSET;
                let owner_callback_addr = owner_context.owner_addr + OWNER_CALLBACK_OFFSET;
                let owner_context_addr = owner_context.owner_addr + OWNER_CONTEXT_OFFSET;
                if fc != FunctionCode::InterruptAck
                    && (addr == owner_gate_addr
                        || addr == owner_callback_addr
                        || addr == owner_callback_addr + 2
                        || addr == owner_context_addr
                        || addr == owner_context_addr + 2
                        || (owner_sig.context_ptr != 0 && addr == producer_flag_addr)
                        || (owner_sig.context_ptr != 0 && addr == producer_flag_addr + 2))
                {
                    report.events.push(make_event(
                        &amiga,
                        tick,
                        if is_read {
                            "tracked_read"
                        } else {
                            "tracked_write"
                        },
                        owner_context.owner_addr,
                    ));
                }
            }
        }
        prev_bus_sig = current_bus_sig;

        if report.events.len() < MAX_EVENTS {
            let kind = match amiga.cpu.instr_start_pc {
                HELPER_GATE_PC => Some("helper_gate_pc"),
                HELPER_GATE_BIT_PC => Some("helper_gate_bit_pc"),
                HELPER_CALLBACK_LOAD_PC => Some("helper_callback_load_pc"),
                HELPER_GATE_SET_PC => Some("helper_gate_set_pc"),
                HELPER_GATE_CLEAR_PC => Some("helper_gate_clear_pc"),
                CALLBACK_WRAPPER_PC => Some("callback_wrapper_pc"),
                CALLBACK_SET_FLAG_PC => Some("callback_set_flag_pc"),
                CALLBACK_DISPATCH_PC => Some("callback_dispatch_pc"),
                PRODUCER_PATH_PC => Some("producer_path_pc"),
                _ => None,
            };
            if let Some(kind) = kind {
                report
                    .events
                    .push(make_event(&amiga, tick, kind, owner_context.owner_addr));
            }
        }

        if !stop_recorded && amiga.cpu.regs.pc == STOP_RESUME_PC && amiga.cpu.ir == 0x4E72 {
            if report.events.len() < MAX_EVENTS {
                report
                    .events
                    .push(make_event(&amiga, tick, "stop", owner_context.owner_addr));
            }
            stop_recorded = true;
        }
    }

    report.final_owner = sample_owner_binding(&amiga, owner_context.owner_addr);
    write_report("owner_callback_trace_kick31_a3000", &report);
}

#[test]
#[ignore]
fn trace_owner_callback_a3000() {
    run_owner_callback_trace();
}
