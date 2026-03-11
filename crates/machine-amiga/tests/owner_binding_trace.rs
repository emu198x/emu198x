//! Focused trace for the A3000 late-wait owner slab setup before the final
//! `Wait(0x10)` helper runs.
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
const WAIT_NODE_HELPER_START: u32 = 0x00FA_4918;
const OWNER_SLAB_BYTES: usize = 0x240;
const MAX_OWNER_SLAB_WRITE_EVENTS: usize = 1024;
const PRE_HELPER_UNKNOWN_ZERO_TRACE_TICKS: u64 = 200_000;
const TASK_NAME_MAX_LEN: usize = 64;
const TASK_NAME_PTR_OFFSET: u32 = 0x0A;
const TASK_STATE_OFFSET: u32 = 0x0F;
const TASK_SIG_WAIT_OFFSET: u32 = 0x16;
const TASK_SIG_RECVD_OFFSET: u32 = 0x1A;
const OWNER_WAIT_LIST_OFFSET: u32 = 0xC0;
const OWNER_RESOURCE_OFFSET: u32 = 0xE0;
const OWNER_RESOURCE_CACHE_OFFSET: u32 = 0xE4;
const OWNER_EXECBASE_PTR_OFFSET: u32 = 0x1A4;
const OWNER_HELPER_ARG_OFFSET: u32 = 0x224;
const OWNER_HELPER_ARG_BYTES: usize = 16;
const SEMAPHORE_NEST_COUNT_OFFSET: u32 = 0x0E;
const SEMAPHORE_OWNER_OFFSET: u32 = 0x28;
const SEMAPHORE_QUEUE_COUNT_OFFSET: u32 = 0x2C;

#[derive(Clone, Copy)]
struct OwnerContext {
    helper_entry_tick: u64,
    owner_addr: u32,
    entry_a0_addr: u32,
}

#[derive(Serialize)]
struct TraceReport {
    rom_path: &'static str,
    helper_entry_tick: u64,
    owner_addr: u32,
    entry_a0_addr: u32,
    first_owner_write_tick: Option<u64>,
    writes_truncated: bool,
    initial_owner: OwnerSlabSnapshot,
    helper_entry_owner: OwnerSlabSnapshot,
    helper_entry_summary: OwnerBindingSummary,
    helper_entry_nonzero_longs: Vec<OwnerLongField>,
    write_events: Vec<OwnerSlabWriteEvent>,
}

#[derive(Clone, Serialize)]
struct OwnerSlabSnapshot {
    addr: u32,
    bytes: Vec<u8>,
}

#[derive(Clone, Serialize)]
struct OwnerBindingSummary {
    wait_list_head: u32,
    wait_list_tail: u32,
    wait_list_tail_pred: u32,
    resource_ptr: u32,
    resource_cache: u32,
    exec_base_ptr: u32,
    helper_arg_bytes: [u8; OWNER_HELPER_ARG_BYTES],
    entry_a0_owner_ptr: u32,
    entry_a0_nest_count: i16,
    entry_a0_queue_count: i16,
}

#[derive(Serialize)]
struct OwnerLongField {
    offset: u32,
    label: &'static str,
    value: u32,
    kind: &'static str,
}

#[derive(Serialize)]
struct OwnerSlabWriteEvent {
    tick: u64,
    addr: u32,
    offset: u32,
    field: &'static str,
    size: &'static str,
    value: u32,
    value_kind: &'static str,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    sr: u16,
    stack_chain: [u32; 4],
    registers: RegisterSnapshot,
    current_entry: Option<TaskSummary>,
    owner: OwnerBindingSummary,
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

#[derive(Clone, Serialize)]
struct TaskSummary {
    addr: u32,
    name: Option<String>,
    state: u8,
    sig_wait: u32,
    sig_recvd: u32,
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
    let data = serde_json::to_vec_pretty(report).expect("serialize owner binding trace report");
    fs::write(&path, data).expect("write owner binding trace report");
    println!("owner binding trace saved to {}", path.display());
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
            pcmcia_card: None,
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

fn read_bus_i16(amiga: &Amiga, addr: u32) -> i16 {
    read_bus_word(amiga, addr) as i16
}

fn read_bytes<const N: usize>(amiga: &Amiga, addr: u32) -> [u8; N] {
    let mut bytes = [0u8; N];
    for (offset, byte) in bytes.iter_mut().enumerate() {
        *byte = read_bus_byte(amiga, addr.wrapping_add(offset as u32));
    }
    bytes
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

fn stack_chain(amiga: &Amiga) -> [u32; 4] {
    let sp = amiga.cpu.regs.a(7);
    [
        read_bus_long(amiga, sp),
        read_bus_long(amiga, sp + 4),
        read_bus_long(amiga, sp + 8),
        read_bus_long(amiga, sp + 12),
    ]
}

fn sample_owner_slab(amiga: &Amiga, owner_addr: u32) -> OwnerSlabSnapshot {
    OwnerSlabSnapshot {
        addr: owner_addr,
        bytes: read_bytes::<OWNER_SLAB_BYTES>(amiga, owner_addr).to_vec(),
    }
}

fn sample_owner_summary(amiga: &Amiga, owner_addr: u32, entry_a0_addr: u32) -> OwnerBindingSummary {
    let wait_list_addr = owner_addr + OWNER_WAIT_LIST_OFFSET;
    OwnerBindingSummary {
        wait_list_head: read_bus_long(amiga, wait_list_addr),
        wait_list_tail: read_bus_long(amiga, wait_list_addr + 4),
        wait_list_tail_pred: read_bus_long(amiga, wait_list_addr + 8),
        resource_ptr: read_bus_long(amiga, owner_addr + OWNER_RESOURCE_OFFSET),
        resource_cache: read_bus_long(amiga, owner_addr + OWNER_RESOURCE_CACHE_OFFSET),
        exec_base_ptr: read_bus_long(amiga, owner_addr + OWNER_EXECBASE_PTR_OFFSET),
        helper_arg_bytes: read_bytes::<OWNER_HELPER_ARG_BYTES>(
            amiga,
            owner_addr + OWNER_HELPER_ARG_OFFSET,
        ),
        entry_a0_owner_ptr: read_bus_long(amiga, entry_a0_addr + SEMAPHORE_OWNER_OFFSET),
        entry_a0_nest_count: read_bus_i16(amiga, entry_a0_addr + SEMAPHORE_NEST_COUNT_OFFSET),
        entry_a0_queue_count: read_bus_i16(amiga, entry_a0_addr + SEMAPHORE_QUEUE_COUNT_OFFSET),
    }
}

fn owner_field_label(offset: u32) -> &'static str {
    match offset {
        0x3A..=0x41 => "queue",
        0xA8..=0xAB => "status_count",
        0xC0..=0xCB => "wait_list",
        0xE0..=0xE7 => "resource",
        0x1A4..=0x1A7 => "exec_base_ptr",
        0x224..=0x233 => "helper_arg",
        _ => "unknown",
    }
}

fn value_kind(amiga: &Amiga, value: u32, owner_addr: u32) -> &'static str {
    if value == 0 {
        "zero"
    } else if value <= 0xFFFF {
        "small"
    } else if (owner_addr..owner_addr + OWNER_SLAB_BYTES as u32).contains(&value) {
        "owner_slab_ptr"
    } else if is_ram_addr(amiga, value) {
        "ram_ptr"
    } else if (0x00F8_0000..0x0100_0000).contains(&value) {
        "rom_ptr"
    } else {
        "other"
    }
}

fn owner_write_is_interesting(
    field: &'static str,
    value: u32,
    tick: u64,
    helper_entry_tick: u64,
) -> bool {
    field != "unknown"
        || value != 0
        || tick.saturating_add(PRE_HELPER_UNKNOWN_ZERO_TRACE_TICKS) >= helper_entry_tick
}

fn sample_owner_nonzero_longs(amiga: &Amiga, owner_addr: u32) -> Vec<OwnerLongField> {
    let mut longs = Vec::with_capacity(OWNER_SLAB_BYTES / 16);
    for offset in (0..OWNER_SLAB_BYTES as u32).step_by(4) {
        let value = read_bus_long(amiga, owner_addr + offset);
        if value != 0 {
            longs.push(OwnerLongField {
                offset,
                label: owner_field_label(offset),
                value,
                kind: value_kind(amiga, value, owner_addr),
            });
        }
    }
    longs
}

fn bus_write_value(addr: u32, is_word: bool, data: u16) -> u32 {
    if is_word {
        u32::from(data)
    } else if addr & 1 == 0 {
        u32::from(data >> 8)
    } else {
        u32::from(data & 0x00FF)
    }
}

fn discover_owner_context() -> OwnerContext {
    let mut amiga = build_amiga().expect("load Kickstart ROM for owner discovery");

    for tick in 0..MAX_BOOT_TICKS {
        amiga.tick();
        if amiga.cpu.instr_start_pc == WAIT_NODE_HELPER_START {
            let owner_addr = amiga.cpu.regs.a(6);
            let entry_a0_addr = amiga.cpu.regs.a(0);
            assert!(
                is_ram_addr(&amiga, owner_addr),
                "owner binding trace should find a RAM-backed owner slab"
            );
            assert!(
                is_ram_addr(&amiga, entry_a0_addr),
                "owner binding trace should find a RAM-backed A0 semaphore"
            );
            return OwnerContext {
                helper_entry_tick: tick,
                owner_addr,
                entry_a0_addr,
            };
        }
    }

    panic!("owner binding trace did not reach the helper");
}

fn run_owner_binding_trace() {
    let owner_context = discover_owner_context();
    let owner_end = owner_context.owner_addr + OWNER_SLAB_BYTES as u32;
    let mut amiga = build_amiga().expect("load Kickstart ROM for owner binding trace");
    let initial_owner = sample_owner_slab(&amiga, owner_context.owner_addr);
    let mut report = TraceReport {
        rom_path: "../../roms/kick31_40_068_a3000.rom",
        helper_entry_tick: owner_context.helper_entry_tick,
        owner_addr: owner_context.owner_addr,
        entry_a0_addr: owner_context.entry_a0_addr,
        first_owner_write_tick: None,
        writes_truncated: false,
        initial_owner,
        helper_entry_owner: OwnerSlabSnapshot {
            addr: owner_context.owner_addr,
            bytes: Vec::new(),
        },
        helper_entry_summary: sample_owner_summary(
            &amiga,
            owner_context.owner_addr,
            owner_context.entry_a0_addr,
        ),
        helper_entry_nonzero_longs: Vec::new(),
        write_events: Vec::with_capacity(MAX_OWNER_SLAB_WRITE_EVENTS.min(128)),
    };
    let mut prev_bus_sig: Option<(u32, u8, bool, bool, Option<u16>)> = None;

    for tick in 0..=owner_context.helper_entry_tick {
        amiga.tick();

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
                let access_len = if is_word { 2 } else { 1 };
                let access_end = addr + access_len;
                if !is_read
                    && fc != FunctionCode::InterruptAck
                    && addr < owner_end
                    && access_end > owner_context.owner_addr
                {
                    if report.first_owner_write_tick.is_none() {
                        report.first_owner_write_tick = Some(tick);
                    }
                    let value =
                        bus_write_value(addr, is_word, data.expect("write bus cycle needs data"));
                    let offset = addr - owner_context.owner_addr;
                    let field = owner_field_label(offset);
                    if !owner_write_is_interesting(
                        field,
                        value,
                        tick,
                        owner_context.helper_entry_tick,
                    ) {
                        continue;
                    }
                    if report.write_events.len() < MAX_OWNER_SLAB_WRITE_EVENTS {
                        report.write_events.push(OwnerSlabWriteEvent {
                            tick,
                            addr,
                            offset,
                            field,
                            size: if is_word { "word" } else { "byte" },
                            value,
                            value_kind: value_kind(&amiga, value, owner_context.owner_addr),
                            pc: amiga.cpu.regs.pc,
                            instr_start_pc: amiga.cpu.instr_start_pc,
                            ir: amiga.cpu.ir,
                            sr: amiga.cpu.regs.sr,
                            stack_chain: stack_chain(&amiga),
                            registers: sample_registers(&amiga),
                            current_entry: sample_current_entry(&amiga, owner_context.owner_addr),
                            owner: sample_owner_summary(
                                &amiga,
                                owner_context.owner_addr,
                                owner_context.entry_a0_addr,
                            ),
                        });
                    } else {
                        report.writes_truncated = true;
                    }
                }
            }
        }
        prev_bus_sig = current_bus_sig;

        if amiga.cpu.instr_start_pc == WAIT_NODE_HELPER_START {
            report.helper_entry_owner = sample_owner_slab(&amiga, owner_context.owner_addr);
            report.helper_entry_summary = sample_owner_summary(
                &amiga,
                owner_context.owner_addr,
                owner_context.entry_a0_addr,
            );
            report.helper_entry_nonzero_longs =
                sample_owner_nonzero_longs(&amiga, owner_context.owner_addr);
            write_report("owner_binding_trace_kick31_a3000", &report);
            return;
        }
    }

    panic!("owner binding trace did not reach the helper entry");
}

#[test]
#[ignore]
fn trace_owner_binding_a3000() {
    run_owner_binding_trace();
}
