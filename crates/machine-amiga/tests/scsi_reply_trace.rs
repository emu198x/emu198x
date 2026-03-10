//! Focused A3000 SCSI timeout/reply trace for the final pre-STOP window.
//!
//! This test is ignored by default because it requires a local Kickstart ROM
//! and writes JSON reports under `test_output/amiga/traces/`.

mod common;

use std::fs;
use std::path::PathBuf;

use machine_amiga::{Amiga, AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};
use serde::Serialize;

use common::load_rom;

const MAX_BOOT_TICKS: u64 = 130_000_000;
const PRE_STOP_TRACE_TICKS: u64 = 100_000;
const POST_STOP_TRACE_TICKS: u64 = 10_000;
const MAX_EVENTS: usize = 256;
const STOP_RESUME_PC: u32 = 0x00F8_1496;
const TASK_NAME_MAX_LEN: usize = 64;
const PROCESS_MSG_PORT_OFFSET: u32 = 0x5A;

const TASK_NAME_PTR_OFFSET: u32 = 0x0A;
const TASK_STATE_OFFSET: u32 = 0x0F;
const TASK_SIG_WAIT_OFFSET: u32 = 0x16;
const TASK_SIG_RECVD_OFFSET: u32 = 0x1A;

const MSGPORT_FLAGS_OFFSET: u32 = 0x0E;
const MSGPORT_SIG_BIT_OFFSET: u32 = 0x0F;
const MSGPORT_SIG_TASK_OFFSET: u32 = 0x10;
const MSGPORT_LIST_HEAD_OFFSET: u32 = 0x14;
const MSGPORT_LIST_TAIL_OFFSET: u32 = 0x18;
const MSGPORT_LIST_TAIL_PRED_OFFSET: u32 = 0x1C;

const MESSAGE_REPLY_PORT_OFFSET: u32 = 0x0E;
const MESSAGE_LENGTH_OFFSET: u32 = 0x12;
const IOREQ_DEVICE_OFFSET: u32 = 0x14;
const IOREQ_UNIT_OFFSET: u32 = 0x18;
const IOREQ_COMMAND_OFFSET: u32 = 0x1C;
const IOREQ_FLAGS_OFFSET: u32 = 0x1E;
const IOREQ_ERROR_OFFSET: u32 = 0x1F;

#[derive(Clone, Copy)]
struct StopContext {
    stop_tick: u64,
    exec_base: u32,
    current_task: u32,
}

#[derive(Serialize)]
struct TraceReport {
    rom_path: &'static str,
    stop_tick: u64,
    window_start_tick: u64,
    exec_base: u32,
    tasks: DiscoveredTasks,
    watched_ioreq_addr: Option<u32>,
    watched_reply_port_addr: Option<u32>,
    events: Vec<TraceEvent>,
}

#[derive(Default, Serialize)]
struct DiscoveredTasks {
    exec_library: u32,
    input_device: Option<u32>,
    scsi_device: Option<u32>,
    scsi_handler: Option<u32>,
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

#[derive(Clone, Serialize, PartialEq, Eq)]
struct MsgPortSnapshot {
    addr: u32,
    flags: u8,
    sig_bit: u8,
    sig_task: u32,
    sig_task_name: Option<String>,
    list_head: u32,
    list_tail: u32,
    list_tail_pred: u32,
    first_message: Option<MessageSnapshot>,
}

#[derive(Clone, Serialize, PartialEq, Eq)]
struct MessageSnapshot {
    addr: u32,
    node_succ: u32,
    node_pred: u32,
    node_type: u8,
    node_pri: i8,
    node_name: Option<String>,
    reply_port: u32,
    reply_port_sig_bit: Option<u8>,
    reply_port_sig_task: Option<u32>,
    reply_port_sig_task_name: Option<String>,
    length: u16,
    io_device: u32,
    io_device_name: Option<String>,
    io_unit: u32,
    io_command: u16,
    io_flags: u8,
    io_error: i8,
    io_actual: u32,
    io_length: u32,
    io_data: u32,
    io_offset: u32,
}

#[derive(Clone, Serialize, PartialEq, Eq)]
struct TaskSnapshot {
    addr: u32,
    name: Option<String>,
    state: u8,
    sig_wait: u32,
    sig_recvd: u32,
    port: Option<MsgPortSnapshot>,
}

#[derive(Serialize)]
struct TraceEvent {
    tick: u64,
    kind: &'static str,
    subject: &'static str,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    sr: u16,
    intena: u16,
    intreq: u16,
    regs: RegisterSnapshot,
    dmac: DmacSnapshot,
    current_entry: Option<TaskSnapshot>,
    exec_library: TaskSnapshot,
    input_device: Option<TaskSnapshot>,
    scsi_device: Option<TaskSnapshot>,
    scsi_handler: Option<TaskSnapshot>,
    watched_ioreq: Option<MessageSnapshot>,
    watched_reply_port: Option<MsgPortSnapshot>,
    watched_ioreq_regs: Vec<&'static str>,
    watched_reply_port_regs: Vec<&'static str>,
    a0_ioreq: Option<MessageSnapshot>,
    a1_ioreq: Option<MessageSnapshot>,
    a2_ioreq: Option<MessageSnapshot>,
    a3_ioreq: Option<MessageSnapshot>,
}

#[derive(Clone, Copy, Serialize)]
struct RegisterSnapshot {
    d0: u32,
    d1: u32,
    a0: u32,
    a1: u32,
    a2: u32,
    a3: u32,
    a6: u32,
    a7: u32,
    stack_return_pc: u32,
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
    let data = serde_json::to_vec_pretty(report).expect("serialize scsi reply trace report");
    fs::write(&path, data).expect("write scsi reply trace report");
    println!("scsi reply trace saved to {}", path.display());
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

fn sample_task_name(amiga: &Amiga, task: u32) -> Option<String> {
    read_c_string(
        amiga,
        read_bus_long(amiga, task + TASK_NAME_PTR_OFFSET),
        TASK_NAME_MAX_LEN,
    )
}

fn sample_message(amiga: &Amiga, msg_addr: u32) -> Option<MessageSnapshot> {
    if msg_addr == 0 || !is_ram_addr(amiga, msg_addr) {
        return None;
    }

    let reply_port = read_bus_long(amiga, msg_addr + MESSAGE_REPLY_PORT_OFFSET);
    let reply_port_sig_bit = if reply_port != 0 && is_ram_addr(amiga, reply_port) {
        Some(read_bus_byte(amiga, reply_port + MSGPORT_SIG_BIT_OFFSET))
    } else {
        None
    };
    let reply_port_sig_task = if reply_port != 0 && is_ram_addr(amiga, reply_port) {
        Some(read_bus_long(amiga, reply_port + MSGPORT_SIG_TASK_OFFSET))
    } else {
        None
    };
    let reply_port_sig_task_name = reply_port_sig_task.and_then(|task| {
        if is_ram_addr(amiga, task) {
            sample_task_name(amiga, task)
        } else {
            None
        }
    });

    let io_device = read_bus_long(amiga, msg_addr + IOREQ_DEVICE_OFFSET);
    let node_name_ptr = read_bus_long(amiga, msg_addr + TASK_NAME_PTR_OFFSET);

    Some(MessageSnapshot {
        addr: msg_addr,
        node_succ: read_bus_long(amiga, msg_addr),
        node_pred: read_bus_long(amiga, msg_addr + 0x04),
        node_type: read_bus_byte(amiga, msg_addr + 0x08),
        node_pri: read_bus_byte(amiga, msg_addr + 0x09) as i8,
        node_name: read_c_string(amiga, node_name_ptr, TASK_NAME_MAX_LEN),
        reply_port,
        reply_port_sig_bit,
        reply_port_sig_task,
        reply_port_sig_task_name,
        length: read_bus_word(amiga, msg_addr + MESSAGE_LENGTH_OFFSET),
        io_device,
        io_device_name: if io_device != 0 && is_ram_addr(amiga, io_device) {
            read_c_string(
                amiga,
                read_bus_long(amiga, io_device + TASK_NAME_PTR_OFFSET),
                TASK_NAME_MAX_LEN,
            )
        } else {
            None
        },
        io_unit: read_bus_long(amiga, msg_addr + IOREQ_UNIT_OFFSET),
        io_command: read_bus_word(amiga, msg_addr + IOREQ_COMMAND_OFFSET),
        io_flags: read_bus_byte(amiga, msg_addr + IOREQ_FLAGS_OFFSET),
        io_error: read_bus_byte(amiga, msg_addr + IOREQ_ERROR_OFFSET) as i8,
        io_actual: read_bus_long(amiga, msg_addr + 0x20),
        io_length: read_bus_long(amiga, msg_addr + 0x24),
        io_data: read_bus_long(amiga, msg_addr + 0x28),
        io_offset: read_bus_long(amiga, msg_addr + 0x2C),
    })
}

fn sample_ioreq_candidate(amiga: &Amiga, addr: u32) -> Option<MessageSnapshot> {
    if addr == 0 || !is_ram_addr(amiga, addr) {
        return None;
    }

    let io_device = read_bus_long(amiga, addr + IOREQ_DEVICE_OFFSET);
    if io_device == 0 || !is_ram_addr(amiga, io_device) {
        return None;
    }

    let device_name = read_c_string(
        amiga,
        read_bus_long(amiga, io_device + TASK_NAME_PTR_OFFSET),
        TASK_NAME_MAX_LEN,
    )?;
    if !device_name.contains("device") {
        return None;
    }

    sample_message(amiga, addr)
}

fn sample_msg_port_at(amiga: &Amiga, port_addr: u32) -> Option<MsgPortSnapshot> {
    if port_addr == 0 || !is_ram_addr(amiga, port_addr) {
        return None;
    }

    let sig_bit = read_bus_byte(amiga, port_addr + MSGPORT_SIG_BIT_OFFSET);
    let sig_task = read_bus_long(amiga, port_addr + MSGPORT_SIG_TASK_OFFSET);
    let list_head = read_bus_long(amiga, port_addr + MSGPORT_LIST_HEAD_OFFSET);
    let list_tail = read_bus_long(amiga, port_addr + MSGPORT_LIST_TAIL_OFFSET);
    let list_tail_pred = read_bus_long(amiga, port_addr + MSGPORT_LIST_TAIL_PRED_OFFSET);
    let empty_list_sentinel = port_addr + MSGPORT_LIST_TAIL_OFFSET;

    if sig_bit >= 32
        || list_tail != 0
        || (sig_task != 0 && !is_ram_addr(amiga, sig_task))
        || (list_head != 0 && list_head != empty_list_sentinel && !is_ram_addr(amiga, list_head))
        || (list_tail_pred != 0
            && list_tail_pred != port_addr + MSGPORT_LIST_HEAD_OFFSET
            && !is_ram_addr(amiga, list_tail_pred))
    {
        return None;
    }

    let first_message = if list_head == 0 || list_head == empty_list_sentinel {
        None
    } else {
        sample_message(amiga, list_head)
    };

    Some(MsgPortSnapshot {
        addr: port_addr,
        flags: read_bus_byte(amiga, port_addr + MSGPORT_FLAGS_OFFSET),
        sig_bit,
        sig_task,
        sig_task_name: if sig_task != 0 {
            sample_task_name(amiga, sig_task)
        } else {
            None
        },
        list_head,
        list_tail,
        list_tail_pred,
        first_message,
    })
}

fn sample_process_msg_port(amiga: &Amiga, task: u32) -> Option<MsgPortSnapshot> {
    sample_msg_port_at(amiga, task + PROCESS_MSG_PORT_OFFSET)
}

fn sample_task(amiga: &Amiga, task: u32) -> Option<TaskSnapshot> {
    if task == 0 || !is_ram_addr(amiga, task) {
        return None;
    }

    Some(TaskSnapshot {
        addr: task,
        name: sample_task_name(amiga, task),
        state: read_bus_byte(amiga, task + TASK_STATE_OFFSET),
        sig_wait: read_bus_long(amiga, task + TASK_SIG_WAIT_OFFSET),
        sig_recvd: read_bus_long(amiga, task + TASK_SIG_RECVD_OFFSET),
        port: sample_process_msg_port(amiga, task),
    })
}

fn discover_stop_context() -> StopContext {
    let mut amiga = build_amiga().expect("load Kickstart ROM for stop discovery");

    for tick in 0..MAX_BOOT_TICKS {
        amiga.tick();
        if amiga.cpu.regs.pc == STOP_RESUME_PC && amiga.cpu.ir == 0x4E72 {
            return StopContext {
                stop_tick: tick,
                exec_base: amiga.cpu.regs.a[6],
                current_task: amiga.cpu.regs.a[1],
            };
        }
    }

    panic!("did not reach STOP loop while discovering SCSI reply trace context");
}

fn matches_named_task(snapshot: &TaskSnapshot, expected: &str) -> bool {
    snapshot.name.as_deref() == Some(expected)
}

fn discover_watched_ioreq(
    a0_ioreq: &Option<MessageSnapshot>,
    a1_ioreq: &Option<MessageSnapshot>,
    a2_ioreq: &Option<MessageSnapshot>,
    a3_ioreq: &Option<MessageSnapshot>,
    exec_task: u32,
) -> Option<MessageSnapshot> {
    [
        a0_ioreq.as_ref(),
        a1_ioreq.as_ref(),
        a2_ioreq.as_ref(),
        a3_ioreq.as_ref(),
    ]
    .into_iter()
    .flatten()
    .find(|msg| {
        msg.reply_port != 0
            && msg.reply_port_sig_task == Some(exec_task)
            && msg.reply_port_sig_bit == Some(4)
            && msg.io_device_name.as_deref() == Some("scsi.device")
    })
    .cloned()
}

fn matching_address_registers(regs: &RegisterSnapshot, target: Option<u32>) -> Vec<&'static str> {
    let Some(target) = target else {
        return Vec::new();
    };
    if target == 0 {
        return Vec::new();
    }

    let mut names = Vec::with_capacity(4);
    if regs.a0 == target {
        names.push("a0");
    }
    if regs.a1 == target {
        names.push("a1");
    }
    if regs.a2 == target {
        names.push("a2");
    }
    if regs.a3 == target {
        names.push("a3");
    }
    names
}

fn register_touch_signature(ioreq_regs: &[&'static str], reply_port_regs: &[&'static str]) -> u8 {
    let mut mask = 0u8;
    for reg in ioreq_regs {
        mask |= match *reg {
            "a0" => 0x01,
            "a1" => 0x02,
            "a2" => 0x04,
            "a3" => 0x08,
            _ => 0,
        };
    }
    for reg in reply_port_regs {
        mask |= match *reg {
            "a0" => 0x10,
            "a1" => 0x20,
            "a2" => 0x40,
            "a3" => 0x80,
            _ => 0,
        };
    }
    mask
}

fn make_event(
    amiga: &Amiga,
    tick: u64,
    kind: &'static str,
    subject: &'static str,
    dmac: DmacSnapshot,
    current_entry: Option<TaskSnapshot>,
    exec_library: &TaskSnapshot,
    input_device: &Option<TaskSnapshot>,
    scsi_device: &Option<TaskSnapshot>,
    scsi_handler: &Option<TaskSnapshot>,
    watched_ioreq_addr: Option<u32>,
    watched_reply_port_addr: Option<u32>,
) -> TraceEvent {
    let regs = RegisterSnapshot {
        d0: amiga.cpu.regs.d[0],
        d1: amiga.cpu.regs.d[1],
        a0: amiga.cpu.regs.a(0),
        a1: amiga.cpu.regs.a(1),
        a2: amiga.cpu.regs.a(2),
        a3: amiga.cpu.regs.a(3),
        a6: amiga.cpu.regs.a(6),
        a7: amiga.cpu.regs.a(7),
        stack_return_pc: read_bus_long(amiga, amiga.cpu.regs.a(7)),
    };
    let watched_ioreq = watched_ioreq_addr.and_then(|addr| sample_message(amiga, addr));
    let watched_reply_port =
        watched_reply_port_addr.and_then(|addr| sample_msg_port_at(amiga, addr));
    let watched_ioreq_regs = matching_address_registers(&regs, watched_ioreq_addr);
    let watched_reply_port_regs = matching_address_registers(&regs, watched_reply_port_addr);

    TraceEvent {
        tick,
        kind,
        subject,
        pc: amiga.cpu.regs.pc,
        instr_start_pc: amiga.cpu.instr_start_pc,
        ir: amiga.cpu.ir,
        sr: amiga.cpu.regs.sr,
        intena: amiga.paula.intena,
        intreq: amiga.paula.intreq,
        regs,
        dmac,
        current_entry,
        exec_library: exec_library.clone(),
        input_device: input_device.clone(),
        scsi_device: scsi_device.clone(),
        scsi_handler: scsi_handler.clone(),
        watched_ioreq,
        watched_reply_port,
        watched_ioreq_regs,
        watched_reply_port_regs,
        a0_ioreq: sample_ioreq_candidate(amiga, regs.a0),
        a1_ioreq: sample_ioreq_candidate(amiga, regs.a1),
        a2_ioreq: sample_ioreq_candidate(amiga, regs.a2),
        a3_ioreq: sample_ioreq_candidate(amiga, regs.a3),
    }
}

fn run_scsi_reply_trace() {
    let context = discover_stop_context();
    let window_start_tick = context.stop_tick.saturating_sub(PRE_STOP_TRACE_TICKS);
    let window_end_tick = context.stop_tick.saturating_add(POST_STOP_TRACE_TICKS);

    let mut amiga = build_amiga().expect("load Kickstart ROM for scsi reply trace");
    let mut report = TraceReport {
        rom_path: "../../roms/kick31_40_068_a3000.rom",
        stop_tick: context.stop_tick,
        window_start_tick,
        exec_base: context.exec_base,
        tasks: DiscoveredTasks {
            exec_library: context.current_task,
            input_device: None,
            scsi_device: None,
            scsi_handler: None,
        },
        watched_ioreq_addr: None,
        watched_reply_port_addr: None,
        events: Vec::new(),
    };

    let mut prev_current_entry_addr = 0u32;
    let mut prev_dmac: Option<DmacSnapshot> = None;
    let mut prev_exec_snapshot: Option<TaskSnapshot> = None;
    let mut prev_input_snapshot: Option<TaskSnapshot> = None;
    let mut prev_scsi_snapshot: Option<TaskSnapshot> = None;
    let mut prev_handler_snapshot: Option<TaskSnapshot> = None;
    let mut watched_ioreq_addr = None;
    let mut watched_reply_port_addr = None;
    let mut prev_watched_ioreq: Option<MessageSnapshot> = None;
    let mut prev_watched_reply_port: Option<MsgPortSnapshot> = None;
    let mut prev_touch_signature = 0u8;

    for tick in 0..=window_end_tick {
        amiga.tick();

        if tick < window_start_tick {
            continue;
        }

        let dmac = sample_dmac(&amiga);
        let current_entry_addr = read_bus_long(&amiga, context.exec_base + 0x114);
        let current_entry_snapshot = sample_task(&amiga, current_entry_addr);

        if let Some(snapshot) = &current_entry_snapshot {
            if matches_named_task(snapshot, "input.device") {
                report.tasks.input_device.get_or_insert(snapshot.addr);
            } else if matches_named_task(snapshot, "scsi.device") {
                report.tasks.scsi_device.get_or_insert(snapshot.addr);
            } else if matches_named_task(snapshot, "SCSI handler") {
                report.tasks.scsi_handler.get_or_insert(snapshot.addr);
            }
        }

        let exec_snapshot = sample_task(&amiga, report.tasks.exec_library)
            .expect("exec.library task should remain accessible");
        let input_snapshot = report
            .tasks
            .input_device
            .and_then(|addr| sample_task(&amiga, addr));
        let scsi_snapshot = report
            .tasks
            .scsi_device
            .and_then(|addr| sample_task(&amiga, addr));
        let handler_snapshot = report
            .tasks
            .scsi_handler
            .and_then(|addr| sample_task(&amiga, addr));
        let regs = RegisterSnapshot {
            d0: amiga.cpu.regs.d[0],
            d1: amiga.cpu.regs.d[1],
            a0: amiga.cpu.regs.a(0),
            a1: amiga.cpu.regs.a(1),
            a2: amiga.cpu.regs.a(2),
            a3: amiga.cpu.regs.a(3),
            a6: amiga.cpu.regs.a(6),
            a7: amiga.cpu.regs.a(7),
            stack_return_pc: read_bus_long(&amiga, amiga.cpu.regs.a(7)),
        };
        let a0_ioreq = sample_ioreq_candidate(&amiga, regs.a0);
        let a1_ioreq = sample_ioreq_candidate(&amiga, regs.a1);
        let a2_ioreq = sample_ioreq_candidate(&amiga, regs.a2);
        let a3_ioreq = sample_ioreq_candidate(&amiga, regs.a3);

        if watched_ioreq_addr.is_none()
            && let Some(watched_ioreq) = discover_watched_ioreq(
                &a0_ioreq,
                &a1_ioreq,
                &a2_ioreq,
                &a3_ioreq,
                report.tasks.exec_library,
            )
        {
            watched_ioreq_addr = Some(watched_ioreq.addr);
            watched_reply_port_addr = Some(watched_ioreq.reply_port);
            report.watched_ioreq_addr = watched_ioreq_addr;
            report.watched_reply_port_addr = watched_reply_port_addr;
        }

        if report.events.is_empty() {
            report.events.push(make_event(
                &amiga,
                tick,
                "window_start",
                "baseline",
                dmac,
                current_entry_snapshot.clone(),
                &exec_snapshot,
                &input_snapshot,
                &scsi_snapshot,
                &handler_snapshot,
                watched_ioreq_addr,
                watched_reply_port_addr,
            ));
        }

        let watched_ioreq = watched_ioreq_addr.and_then(|addr| sample_message(&amiga, addr));
        let watched_reply_port =
            watched_reply_port_addr.and_then(|addr| sample_msg_port_at(&amiga, addr));
        let touch_signature = register_touch_signature(
            &matching_address_registers(&regs, watched_ioreq_addr),
            &matching_address_registers(&regs, watched_reply_port_addr),
        );

        if prev_watched_ioreq.is_none()
            && watched_ioreq.is_some()
            && report.events.len() < MAX_EVENTS
        {
            report.events.push(make_event(
                &amiga,
                tick,
                "watch_discovered",
                "watched_ioreq",
                dmac,
                current_entry_snapshot.clone(),
                &exec_snapshot,
                &input_snapshot,
                &scsi_snapshot,
                &handler_snapshot,
                watched_ioreq_addr,
                watched_reply_port_addr,
            ));
            prev_watched_ioreq = watched_ioreq.clone();
        }
        if prev_watched_ioreq.as_ref() != watched_ioreq.as_ref() && report.events.len() < MAX_EVENTS
        {
            report.events.push(make_event(
                &amiga,
                tick,
                "watch_change",
                "watched_ioreq",
                dmac,
                current_entry_snapshot.clone(),
                &exec_snapshot,
                &input_snapshot,
                &scsi_snapshot,
                &handler_snapshot,
                watched_ioreq_addr,
                watched_reply_port_addr,
            ));
            prev_watched_ioreq = watched_ioreq.clone();
        }
        if prev_watched_reply_port.as_ref() != watched_reply_port.as_ref()
            && report.events.len() < MAX_EVENTS
        {
            report.events.push(make_event(
                &amiga,
                tick,
                "watch_change",
                "reply_port",
                dmac,
                current_entry_snapshot.clone(),
                &exec_snapshot,
                &input_snapshot,
                &scsi_snapshot,
                &handler_snapshot,
                watched_ioreq_addr,
                watched_reply_port_addr,
            ));
            prev_watched_reply_port = watched_reply_port.clone();
        }
        if touch_signature != 0
            && touch_signature != prev_touch_signature
            && report.events.len() < MAX_EVENTS
        {
            report.events.push(make_event(
                &amiga,
                tick,
                "watch_touch",
                "watched_ioreq",
                dmac,
                current_entry_snapshot.clone(),
                &exec_snapshot,
                &input_snapshot,
                &scsi_snapshot,
                &handler_snapshot,
                watched_ioreq_addr,
                watched_reply_port_addr,
            ));
        }
        prev_touch_signature = touch_signature;

        if current_entry_addr != prev_current_entry_addr && report.events.len() < MAX_EVENTS {
            report.events.push(make_event(
                &amiga,
                tick,
                "current_entry_change",
                "current_entry",
                dmac,
                current_entry_snapshot.clone(),
                &exec_snapshot,
                &input_snapshot,
                &scsi_snapshot,
                &handler_snapshot,
                watched_ioreq_addr,
                watched_reply_port_addr,
            ));
            prev_current_entry_addr = current_entry_addr;
        }

        if prev_dmac != Some(dmac) && report.events.len() < MAX_EVENTS {
            report.events.push(make_event(
                &amiga,
                tick,
                "dmac_change",
                "dmac",
                dmac,
                current_entry_snapshot.clone(),
                &exec_snapshot,
                &input_snapshot,
                &scsi_snapshot,
                &handler_snapshot,
                watched_ioreq_addr,
                watched_reply_port_addr,
            ));
            prev_dmac = Some(dmac);
        }

        if prev_exec_snapshot.as_ref() != Some(&exec_snapshot) && report.events.len() < MAX_EVENTS {
            report.events.push(make_event(
                &amiga,
                tick,
                "task_change",
                "exec_library",
                dmac,
                current_entry_snapshot.clone(),
                &exec_snapshot,
                &input_snapshot,
                &scsi_snapshot,
                &handler_snapshot,
                watched_ioreq_addr,
                watched_reply_port_addr,
            ));
            prev_exec_snapshot = Some(exec_snapshot.clone());
        }

        if prev_input_snapshot.as_ref() != input_snapshot.as_ref()
            && report.events.len() < MAX_EVENTS
        {
            report.events.push(make_event(
                &amiga,
                tick,
                "task_change",
                "input_device",
                dmac,
                current_entry_snapshot.clone(),
                &exec_snapshot,
                &input_snapshot,
                &scsi_snapshot,
                &handler_snapshot,
                watched_ioreq_addr,
                watched_reply_port_addr,
            ));
            prev_input_snapshot = input_snapshot.clone();
        }

        if prev_scsi_snapshot.as_ref() != scsi_snapshot.as_ref() && report.events.len() < MAX_EVENTS
        {
            report.events.push(make_event(
                &amiga,
                tick,
                "task_change",
                "scsi_device",
                dmac,
                current_entry_snapshot.clone(),
                &exec_snapshot,
                &input_snapshot,
                &scsi_snapshot,
                &handler_snapshot,
                watched_ioreq_addr,
                watched_reply_port_addr,
            ));
            prev_scsi_snapshot = scsi_snapshot.clone();
        }

        if prev_handler_snapshot.as_ref() != handler_snapshot.as_ref()
            && report.events.len() < MAX_EVENTS
        {
            report.events.push(make_event(
                &amiga,
                tick,
                "task_change",
                "scsi_handler",
                dmac,
                current_entry_snapshot.clone(),
                &exec_snapshot,
                &input_snapshot,
                &scsi_snapshot,
                &handler_snapshot,
                watched_ioreq_addr,
                watched_reply_port_addr,
            ));
            prev_handler_snapshot = handler_snapshot.clone();
        }
    }

    write_report("scsi_reply_trace_kick31_a3000", &report);
}

#[test]
#[ignore]
fn trace_scsi_reply_a3000() {
    run_scsi_reply_trace();
}
