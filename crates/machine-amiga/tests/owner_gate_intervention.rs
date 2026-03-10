//! Focused A3000 intervention test for the late owner gate that remains set
//! before the final `Wait(0x10)` stall.
//!
//! This test is ignored by default because it requires a local Kickstart ROM
//! and writes JSON under `test_output/amiga/traces/`.

mod common;

use std::fs;
use std::path::PathBuf;

use machine_amiga::{Amiga, AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};
use motorola_68000::bus::FunctionCode;
use serde::Serialize;

use common::load_rom;

const MAX_BOOT_TICKS: u64 = 60_000_000;
const TRACE_TICKS_AFTER_INTERVENTION: u64 = 8_000_000;
const MAX_EVENTS: usize = 128;

const STOP_RESUME_PC: u32 = 0x00F8_1496;
const WAIT_NODE_HELPER_START: u32 = 0x00FA_4918;
const BLITTER_WAIT_LOOP_PC: u32 = 0x00FA_4906;
const HELPER_GATE_CLEAR_PC: u32 = 0x00FA_5716;
const CALLBACK_WRAPPER_PC: u32 = 0x00FC_DB92;
const CALLBACK_SET_FLAG_PC: u32 = 0x00FD_F6D8;
const CALLBACK_DISPATCH_PC: u32 = 0x00FD_F6E4;
const PRODUCER_PATH_PC: u32 = 0x00FD_F310;
const MAX_BLITTER_WRITES: usize = 64;
const MAX_BLITTER_STATE_CHANGES: usize = 48;

const TASK_NAME_MAX_LEN: usize = 64;
const TASK_NAME_PTR_OFFSET: u32 = 0x0A;
const TASK_STATE_OFFSET: u32 = 0x0F;
const TASK_SIG_WAIT_OFFSET: u32 = 0x16;
const TASK_SIG_RECVD_OFFSET: u32 = 0x1A;

const OWNER_ACTIVE_BITS_OFFSET: u32 = 0x0A6;
const OWNER_PENDING_BITS_OFFSET: u32 = 0x1BC;
const OWNER_EXECBASE_PTR_OFFSET: u32 = 0x1A4;
const OWNER_GATE_BITS_OFFSET: u32 = 0x1C0;
const OWNER_CALLBACK_OFFSET: u32 = 0x1D0;
const OWNER_CONTEXT_OFFSET: u32 = 0x1D4;

const PRODUCER_WAKE_FLAG_OFFSET: u32 = 0x0BE2;

#[derive(Clone, Copy)]
struct OwnerContext {
    owner_addr: u32,
}

#[derive(Clone, Serialize)]
struct TaskSummary {
    addr: u32,
    name: Option<String>,
    state: u8,
    sig_wait: u32,
    sig_recvd: u32,
}

#[derive(Clone, Copy, Serialize, PartialEq, Eq)]
struct OwnerSig {
    active_bits: u8,
    pending_bits: u8,
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
    owner: OwnerSig,
    current_entry: Option<TaskSummary>,
}

#[derive(Clone, Copy, Serialize, PartialEq, Eq)]
struct BlitterSig {
    blitter_busy: bool,
    blitter_ccks_remaining: u32,
    dmacon: u16,
    intena: u16,
    intreq: u16,
    bltsize: u16,
    bltsizv_ecs: u16,
    bltsizh_ecs: u16,
}

#[derive(Serialize)]
struct BlitterStateChange {
    tick: u64,
    pc: u32,
    instr_start_pc: u32,
    blitter: BlitterSig,
}

#[derive(Serialize)]
struct BlitterWriteEvent {
    tick: u64,
    register: &'static str,
    addr: u32,
    is_word: bool,
    raw_data: u16,
    effective_data: u16,
    pc: u32,
    instr_start_pc: u32,
    blitter: BlitterSig,
}

#[derive(Serialize)]
struct InterventionReport {
    rom_path: &'static str,
    owner_addr: u32,
    intervention_tick: Option<u64>,
    intervention_old_gate_bits: Option<u8>,
    intervention_new_gate_bits: Option<u8>,
    first_producer_path_tick: Option<u64>,
    first_callback_wrapper_tick: Option<u64>,
    first_callback_set_flag_tick: Option<u64>,
    first_callback_dispatch_tick: Option<u64>,
    first_gate_clear_tick: Option<u64>,
    first_blitter_wait_tick: Option<u64>,
    first_exec_signal_tick: Option<u64>,
    first_stop_tick: Option<u64>,
    final_tick: u64,
    final_pc: u32,
    final_instr_start_pc: u32,
    final_dmacon: u16,
    final_bplcon0: u16,
    final_blitter_busy: bool,
    final_blitter_ccks_remaining: u32,
    final_bltsize: u16,
    final_bltsizv_ecs: u16,
    final_bltsizh_ecs: u16,
    final_owner: OwnerSig,
    final_current_entry: Option<TaskSummary>,
    events: Vec<TraceEvent>,
    blitter_state_changes: Vec<BlitterStateChange>,
    blitter_writes: Vec<BlitterWriteEvent>,
}

fn report_path(report_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test_output/amiga/traces")
        .join(format!("{report_name}.json"))
}

fn write_report(report_name: &str, report: &InterventionReport) {
    let path = report_path(report_name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create trace output directory");
    }
    let data = serde_json::to_vec_pretty(report).expect("serialize intervention report");
    fs::write(&path, data).expect("write intervention report");
    println!("owner gate intervention trace saved to {}", path.display());
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

fn write_bus_byte(amiga: &mut Amiga, addr: u32, value: u8) {
    if !amiga.memory.fast_ram.is_empty() {
        let base = amiga.memory.fast_ram_base;
        let end = base.wrapping_add(amiga.memory.fast_ram.len() as u32);
        if addr >= base && addr < end {
            let offset = (addr - base) & amiga.memory.fast_ram_mask;
            amiga.memory.fast_ram[offset as usize] = value;
            return;
        }
    }
    amiga.memory.write_byte(addr, value);
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

fn sample_owner(amiga: &Amiga, owner_addr: u32) -> OwnerSig {
    let callback_ptr = read_bus_long(amiga, owner_addr + OWNER_CALLBACK_OFFSET);
    let context_ptr = read_bus_long(amiga, owner_addr + OWNER_CONTEXT_OFFSET);
    let producer_wake_flag = if is_ram_addr(amiga, context_ptr) {
        read_bus_long(amiga, context_ptr + PRODUCER_WAKE_FLAG_OFFSET)
    } else {
        0
    };
    OwnerSig {
        active_bits: read_bus_byte(amiga, owner_addr + OWNER_ACTIVE_BITS_OFFSET),
        pending_bits: read_bus_byte(amiga, owner_addr + OWNER_PENDING_BITS_OFFSET),
        gate_bits: read_bus_byte(amiga, owner_addr + OWNER_GATE_BITS_OFFSET),
        callback_ptr,
        context_ptr,
        producer_wake_flag,
    }
}

fn sample_blitter(amiga: &Amiga) -> BlitterSig {
    BlitterSig {
        blitter_busy: amiga.agnus.blitter_busy,
        blitter_ccks_remaining: amiga.agnus.blitter_ccks_remaining,
        dmacon: amiga.agnus.dmacon,
        intena: amiga.paula.intena,
        intreq: amiga.paula.intreq,
        bltsize: amiga.agnus.bltsize,
        bltsizv_ecs: amiga.agnus.bltsizv_ecs,
        bltsizh_ecs: amiga.agnus.bltsizh_ecs,
    }
}

fn make_event(amiga: &Amiga, tick: u64, kind: &'static str, owner_addr: u32) -> TraceEvent {
    TraceEvent {
        tick,
        kind,
        pc: amiga.cpu.regs.pc,
        instr_start_pc: amiga.cpu.instr_start_pc,
        owner: sample_owner(amiga, owner_addr),
        current_entry: sample_current_entry(amiga, owner_addr),
    }
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

fn discover_owner_context() -> OwnerContext {
    let mut amiga = build_amiga().expect("load Kickstart ROM for owner discovery");

    for _tick in 0..MAX_BOOT_TICKS {
        amiga.tick();
        if amiga.cpu.instr_start_pc == WAIT_NODE_HELPER_START {
            let owner_addr = amiga.cpu.regs.a[6];
            assert!(
                is_ram_addr(&amiga, owner_addr),
                "owner gate intervention should find a RAM-backed owner slab"
            );
            return OwnerContext { owner_addr };
        }
    }

    panic!("owner gate intervention did not reach the helper");
}

fn run_owner_gate_intervention() {
    let owner_context = discover_owner_context();
    let mut amiga = build_amiga().expect("load Kickstart ROM for intervention trace");
    let mut report = InterventionReport {
        rom_path: "../../roms/kick31_40_068_a3000.rom",
        owner_addr: owner_context.owner_addr,
        intervention_tick: None,
        intervention_old_gate_bits: None,
        intervention_new_gate_bits: None,
        first_producer_path_tick: None,
        first_callback_wrapper_tick: None,
        first_callback_set_flag_tick: None,
        first_callback_dispatch_tick: None,
        first_gate_clear_tick: None,
        first_blitter_wait_tick: None,
        first_exec_signal_tick: None,
        first_stop_tick: None,
        final_tick: 0,
        final_pc: 0,
        final_instr_start_pc: 0,
        final_dmacon: 0,
        final_bplcon0: 0,
        final_blitter_busy: false,
        final_blitter_ccks_remaining: 0,
        final_bltsize: 0,
        final_bltsizv_ecs: 0,
        final_bltsizh_ecs: 0,
        final_owner: OwnerSig {
            active_bits: 0,
            pending_bits: 0,
            gate_bits: 0,
            callback_ptr: 0,
            context_ptr: 0,
            producer_wake_flag: 0,
        },
        final_current_entry: None,
        events: Vec::with_capacity(64),
        blitter_state_changes: Vec::with_capacity(32),
        blitter_writes: Vec::with_capacity(32),
    };
    let mut prev_owner = None;
    let mut prev_blitter = None;
    let mut prev_bus_sig: Option<(u32, u8, bool, bool, Option<u16>)> = None;
    let mut trace_end_tick = MAX_BOOT_TICKS;

    for tick in 0..MAX_BOOT_TICKS {
        amiga.tick();

        let owner = sample_owner(&amiga, owner_context.owner_addr);
        let blitter = sample_blitter(&amiga);
        if report.events.len() < MAX_EVENTS && prev_owner != Some(owner) {
            report.events.push(make_event(
                &amiga,
                tick,
                "owner_change",
                owner_context.owner_addr,
            ));
        }
        prev_owner = Some(owner);
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

        if report.intervention_tick.is_none()
            && (owner.gate_bits & 1) != 0
            && owner.callback_ptr == CALLBACK_WRAPPER_PC
            && owner.context_ptr != 0
        {
            let new_gate_bits = owner.gate_bits & !1;
            write_bus_byte(
                &mut amiga,
                owner_context.owner_addr + OWNER_GATE_BITS_OFFSET,
                new_gate_bits,
            );
            report.intervention_tick = Some(tick);
            report.intervention_old_gate_bits = Some(owner.gate_bits);
            report.intervention_new_gate_bits = Some(read_bus_byte(
                &amiga,
                owner_context.owner_addr + OWNER_GATE_BITS_OFFSET,
            ));
            report.events.push(make_event(
                &amiga,
                tick,
                "force_clear_gate",
                owner_context.owner_addr,
            ));
            trace_end_tick = tick.saturating_add(TRACE_TICKS_AFTER_INTERVENTION);
        }

        if report.intervention_tick.is_some() {
            if report.first_producer_path_tick.is_none()
                && amiga.cpu.instr_start_pc == PRODUCER_PATH_PC
            {
                report.first_producer_path_tick = Some(tick);
                if report.events.len() < MAX_EVENTS {
                    report.events.push(make_event(
                        &amiga,
                        tick,
                        "producer_path_pc",
                        owner_context.owner_addr,
                    ));
                }
            }

            if report.first_callback_wrapper_tick.is_none()
                && amiga.cpu.instr_start_pc == CALLBACK_WRAPPER_PC
            {
                report.first_callback_wrapper_tick = Some(tick);
                if report.events.len() < MAX_EVENTS {
                    report.events.push(make_event(
                        &amiga,
                        tick,
                        "callback_wrapper_pc",
                        owner_context.owner_addr,
                    ));
                }
            }

            if report.first_callback_set_flag_tick.is_none()
                && amiga.cpu.instr_start_pc == CALLBACK_SET_FLAG_PC
            {
                report.first_callback_set_flag_tick = Some(tick);
                if report.events.len() < MAX_EVENTS {
                    report.events.push(make_event(
                        &amiga,
                        tick,
                        "callback_set_flag_pc",
                        owner_context.owner_addr,
                    ));
                }
            }

            if report.first_callback_dispatch_tick.is_none()
                && amiga.cpu.instr_start_pc == CALLBACK_DISPATCH_PC
            {
                report.first_callback_dispatch_tick = Some(tick);
                if report.events.len() < MAX_EVENTS {
                    report.events.push(make_event(
                        &amiga,
                        tick,
                        "callback_dispatch_pc",
                        owner_context.owner_addr,
                    ));
                }
            }

            if report.first_gate_clear_tick.is_none()
                && amiga.cpu.instr_start_pc == HELPER_GATE_CLEAR_PC
            {
                report.first_gate_clear_tick = Some(tick);
                if report.events.len() < MAX_EVENTS {
                    report.events.push(make_event(
                        &amiga,
                        tick,
                        "helper_gate_clear_pc",
                        owner_context.owner_addr,
                    ));
                }
            }

            if report.first_stop_tick.is_none()
                && amiga.cpu.regs.pc == STOP_RESUME_PC
                && amiga.cpu.ir == 0x4E72
            {
                report.first_stop_tick = Some(tick);
                if report.events.len() < MAX_EVENTS {
                    report
                        .events
                        .push(make_event(&amiga, tick, "stop", owner_context.owner_addr));
                }
            }

            if report.first_blitter_wait_tick.is_none()
                && amiga.cpu.instr_start_pc == BLITTER_WAIT_LOOP_PC
            {
                report.first_blitter_wait_tick = Some(tick);
                if report.events.len() < MAX_EVENTS {
                    report.events.push(make_event(
                        &amiga,
                        tick,
                        "blitter_wait_loop",
                        owner_context.owner_addr,
                    ));
                }
            }

            if report.first_exec_signal_tick.is_none() {
                if let Some(current_entry) = sample_current_entry(&amiga, owner_context.owner_addr)
                {
                    if current_entry.name.as_deref() == Some("exec.library")
                        && (current_entry.sig_recvd & 0x10) != 0
                    {
                        report.first_exec_signal_tick = Some(tick);
                        if report.events.len() < MAX_EVENTS {
                            report.events.push(make_event(
                                &amiga,
                                tick,
                                "exec_signal_0x10",
                                owner_context.owner_addr,
                            ));
                        }
                        report.blitter_state_changes.clear();
                        report.blitter_writes.clear();
                        prev_blitter = None;
                        prev_bus_sig = None;
                    }
                }
            }

            if report.first_exec_signal_tick.is_some() {
                if report.blitter_state_changes.len() < MAX_BLITTER_STATE_CHANGES
                    && prev_blitter != Some(blitter)
                {
                    report.blitter_state_changes.push(BlitterStateChange {
                        tick,
                        pc: amiga.cpu.regs.pc,
                        instr_start_pc: amiga.cpu.instr_start_pc,
                        blitter,
                    });
                }

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
                            && report.blitter_writes.len() < MAX_BLITTER_WRITES
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
                                    report.blitter_writes.push(BlitterWriteEvent {
                                        tick,
                                        register,
                                        addr: addr24,
                                        is_word,
                                        raw_data,
                                        effective_data,
                                        pc: amiga.cpu.regs.pc,
                                        instr_start_pc: amiga.cpu.instr_start_pc,
                                        blitter,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        prev_blitter = Some(blitter);
        prev_bus_sig = current_bus_sig;

        if report.intervention_tick.is_some() && tick >= trace_end_tick {
            report.final_tick = tick;
            break;
        }
    }

    assert!(
        report.intervention_tick.is_some(),
        "owner gate intervention never found the late bound owner"
    );

    report.final_pc = amiga.cpu.regs.pc;
    report.final_instr_start_pc = amiga.cpu.instr_start_pc;
    report.final_dmacon = amiga.agnus.dmacon;
    report.final_bplcon0 = amiga.agnus.bplcon0;
    report.final_blitter_busy = amiga.agnus.blitter_busy;
    report.final_blitter_ccks_remaining = amiga.agnus.blitter_ccks_remaining;
    report.final_bltsize = amiga.agnus.bltsize;
    report.final_bltsizv_ecs = amiga.agnus.bltsizv_ecs;
    report.final_bltsizh_ecs = amiga.agnus.bltsizh_ecs;
    report.final_owner = sample_owner(&amiga, owner_context.owner_addr);
    report.final_current_entry = sample_current_entry(&amiga, owner_context.owner_addr);

    write_report("owner_gate_intervention_kick31_a3000", &report);
}

#[test]
#[ignore]
fn trace_owner_gate_intervention_a3000() {
    run_owner_gate_intervention();
}
