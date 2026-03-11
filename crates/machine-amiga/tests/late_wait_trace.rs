//! Focused trace for the late A3000 `Wait(0x10)` stall after the last
//! successful `exec.library` wakeup.
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
const PRE_LAST_SIGNAL_TICKS: u64 = 50_000;
const PRE_HELPER_OWNER_TRACE_TICKS: u64 = 50_000;
const PRE_HANDOFF_TRACE_TICKS: u64 = 4_096;
const PRE_RESOURCE_TRACE_TICKS: u64 = 8_192;
const POST_STOP_TRACE_TICKS: u64 = 10_000;
const PC_SAMPLE_PERIOD: u64 = 2_048;
const MAX_EVENTS: usize = 512;
const MAX_BUS_WRITE_EVENTS: usize = 128;
const MAX_OWNER_WRITE_EVENTS: usize = 128;
const MAX_PRODUCER_BUS_EVENTS: usize = 256;
const MAX_HANDOFF_BUS_EVENTS: usize = 256;
const MAX_HANDOFF_CHANGE_EVENTS: usize = 128;
const MAX_RESOURCE_BUS_EVENTS: usize = 256;
const MAX_RESOURCE_CHANGE_EVENTS: usize = 128;
const TASK_NAME_MAX_LEN: usize = 64;
const TASK_NAME_PTR_OFFSET: u32 = 0x0A;
const TASK_STATE_OFFSET: u32 = 0x0F;
const TASK_SIG_WAIT_OFFSET: u32 = 0x16;
const TASK_SIG_RECVD_OFFSET: u32 = 0x1A;
const NODE_SUCC_OFFSET: u32 = 0x00;
const NODE_PRED_OFFSET: u32 = 0x04;
const NODE_TYPE_OFFSET: u32 = 0x08;
const NODE_PRI_OFFSET: u32 = 0x09;
const NODE_NAME_OFFSET: u32 = 0x0A;
const LIST_HEAD_OFFSET: u32 = 0x00;
const LIST_TAIL_OFFSET: u32 = 0x04;
const LIST_TAIL_PRED_OFFSET: u32 = 0x08;
const OWNER_QUEUE_HEAD_OFFSET: u32 = 0x3A;
const OWNER_QUEUE_TAIL_OFFSET: u32 = 0x3E;
const OWNER_STATUS_OFFSET: u32 = 0xA8;
const OWNER_COUNT_OFFSET: u32 = 0xAA;
const OWNER_WAIT_LIST_OFFSET: u32 = 0xC0;
const OWNER_RESOURCE_OFFSET: u32 = 0xE0;
const OWNER_RESOURCE_CACHE_OFFSET: u32 = 0xE4;
const OWNER_EXECBASE_PTR_OFFSET: u32 = 0x1A4;
const OWNER_HELPER_ARG_OFFSET: u32 = 0x224;
const OWNER_HELPER_ARG_BYTES: usize = 16;
const PRODUCER_OBJECT_BYTES: usize = 32;
const HANDOFF_OBJECT_BYTES: usize = 64;
const RESOURCE_OBJECT_BYTES: usize = 64;
const SEMAPHORE_NEST_COUNT_OFFSET: u32 = 0x0E;
const SEMAPHORE_WAIT_LIST_OFFSET: u32 = 0x10;
const SEMAPHORE_MULTIPLE_LINK_OFFSET: u32 = 0x1C;
const SEMAPHORE_OWNER_OFFSET: u32 = 0x28;
const SEMAPHORE_QUEUE_COUNT_OFFSET: u32 = 0x2C;
const PRODUCER_ENTRY_PC: u32 = 0x00FD_F310;
const PRODUCER_OUTER_END_PC: u32 = 0x00FD_F454;
const PRODUCER_TRACE_START_PC: u32 = 0x00FD_F3C0;
const PRODUCER_TRACE_END_PC: u32 = 0x00FD_F456;
const PRODUCER_WAIT_WRAPPER_PC: u32 = 0x00FD_F42E;
const PRODUCER_FREE_PREV_PC: u32 = 0x00FD_F434;
const PRODUCER_STORE_PENDING_PC: u32 = 0x00FD_F440;
const PRODUCER_RELEASE_SLOT4_PC: u32 = 0x00FD_F444;
const PRODUCER_SELECTED_PTR_OFFSET: u32 = 0x0BD2;
const PRODUCER_PENDING_PTR_OFFSET: u32 = 0x0BD6;
const PRODUCER_SCREEN_PTR_OFFSET: u32 = 0x0BB8;
const PRODUCER_CURRENT_PTR_OFFSET: u32 = 0x0BBE;
const PRODUCER_FALLBACK_PTR_OFFSET: u32 = 0x0BC2;
const PRODUCER_BYTE0_OFFSET: u32 = 0x0BB6;
const PRODUCER_BYTE1_OFFSET: u32 = 0x0BB7;
const PRODUCER_WORD0_OFFSET: u32 = 0x0BDA;
const PRODUCER_WORD1_OFFSET: u32 = 0x0BDC;
const PRODUCER_SOURCE_PTR_OFFSET: u32 = 0x0034;
const PRODUCER_SOURCE_PENDING_PTR_OFFSET: u32 = 0x00D0;
const WAIT_NODE_HELPER_START: u32 = 0x00FA_4918;
const WAIT_NODE_HELPER_END: u32 = 0x00FA_498A;
const WAIT_NODE_SETUP_PC: u32 = 0x00FA_4940;

#[derive(Clone, Copy)]
struct StopContext {
    stop_tick: u64,
    exec_base: u32,
    exec_task: u32,
}

#[derive(Clone, Copy)]
struct SignalWindow {
    last_signal_set_tick: u64,
    last_signal_clear_tick: u64,
}

#[derive(Clone, Copy)]
struct WaitTargets {
    wait_list_addr: u32,
    wait_entry_addr: u32,
}

#[derive(Clone, Copy)]
struct OwnerContext {
    helper_start_tick: u64,
    owner_addr: u32,
    entry_a0_addr: u32,
}

#[derive(Clone, Copy)]
struct ProducerContext {
    base_addr: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct HandoffContext {
    trace_start_tick: u64,
    addr: u32,
    source_addr: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ResourceContext {
    trace_start_tick: u64,
    addr: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ExecStateSig {
    state: u8,
    sig_wait: u32,
    sig_recvd: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ReadyQueueSig {
    head: u32,
    tail: u32,
    tail_pred: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct WaitNodeSig {
    succ: u32,
    pred: u32,
    node_type: u8,
    node_pri: u8,
    node_name: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct WaitListSig {
    head: u32,
    tail: u32,
    tail_pred: u32,
}

#[derive(Serialize)]
struct TraceReport {
    rom_path: &'static str,
    stop_tick: u64,
    helper_entry_tick: u64,
    exec_base: u32,
    exec_task: u32,
    exec_task_name: Option<String>,
    last_signal_set_tick: u64,
    last_signal_clear_tick: u64,
    window_start_tick: u64,
    owner_trace_start_tick: u64,
    final_wait_node_addr: Option<u32>,
    final_wait_list_addr: Option<u32>,
    final_wait_entry_addr: Option<u32>,
    final_owner_addr: Option<u32>,
    final_entry_a0_addr: Option<u32>,
    final_producer_addr: Option<u32>,
    handoff_trace_start_tick: Option<u64>,
    final_handoff_addr: Option<u32>,
    final_handoff_source_addr: Option<u32>,
    final_handoff: Option<HandoffSnapshot>,
    resource_trace_start_tick: Option<u64>,
    final_resource_addr: Option<u32>,
    final_resource: Option<ResourceSnapshot>,
    events: Vec<TraceEvent>,
    bus_write_events: Vec<BusWriteEvent>,
    owner_write_events: Vec<OwnerWriteEvent>,
    producer_bus_events: Vec<ProducerBusEvent>,
    handoff_bus_events: Vec<HandoffBusEvent>,
    handoff_change_events: Vec<HandoffChangeEvent>,
    resource_bus_events: Vec<ResourceBusEvent>,
    resource_change_events: Vec<ResourceChangeEvent>,
    pc_samples: Vec<PcSample>,
}

#[derive(Serialize)]
struct TraceEvent {
    tick: u64,
    kind: &'static str,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    sr: u16,
    intena: u16,
    intreq: u16,
    registers: RegisterSnapshot,
    stack_chain: [u32; 4],
    current_entry: Option<TaskSummary>,
    exec_task: TaskSummary,
    ready_queue: ReadyQueueSnapshot,
    dmac: DmacSnapshot,
    wait_node_addr: Option<u32>,
    wait_list_addr: Option<u32>,
    wait_node: Option<WaitNodeSnapshot>,
    wait_list: Option<WaitListSnapshot>,
    owner_addr: Option<u32>,
    owner: Option<OwnerSnapshot>,
    producer_addr: Option<u32>,
    producer: Option<ProducerSnapshot>,
}

#[derive(Serialize)]
struct PcSample {
    tick: u64,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    current_entry: Option<String>,
    exec_state: u8,
    exec_sig_wait: u32,
    exec_sig_recvd: u32,
    wait_node_addr: Option<u32>,
    wait_list_addr: Option<u32>,
    wait_list_head: Option<u32>,
}

#[derive(Serialize)]
struct BusWriteEvent {
    tick: u64,
    addr: u32,
    size: &'static str,
    value: u32,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    sr: u16,
    current_entry: Option<TaskSummary>,
    registers: RegisterSnapshot,
}

#[derive(Serialize)]
struct OwnerWriteEvent {
    tick: u64,
    target: &'static str,
    addr: u32,
    size: &'static str,
    value: u32,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    sr: u16,
    current_entry: Option<TaskSummary>,
    registers: RegisterSnapshot,
    owner: OwnerSnapshot,
}

#[derive(Serialize)]
struct ProducerBusEvent {
    tick: u64,
    direction: &'static str,
    addr: u32,
    size: &'static str,
    value: Option<u32>,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    sr: u16,
    current_entry: Option<TaskSummary>,
    registers: RegisterSnapshot,
    offset_from_producer: Option<i32>,
    offset_from_a2: Option<i32>,
    offset_from_a5: Option<i32>,
    offset_from_a6: Option<i32>,
}

#[derive(Clone, Serialize, PartialEq, Eq)]
struct HandoffSnapshot {
    addr: u32,
    bytes: Vec<u8>,
}

#[derive(Serialize)]
struct HandoffBusEvent {
    tick: u64,
    direction: &'static str,
    addr: u32,
    size: &'static str,
    value: Option<u32>,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    sr: u16,
    current_entry: Option<TaskSummary>,
    registers: RegisterSnapshot,
    offset_from_handoff: i32,
}

#[derive(Serialize)]
struct HandoffChangeEvent {
    tick: u64,
    kind: &'static str,
    pc: u32,
    instr_start_pc: u32,
    current_entry: Option<TaskSummary>,
    registers: RegisterSnapshot,
    handoff: HandoffSnapshot,
}

#[derive(Clone, Serialize, PartialEq, Eq)]
struct ResourceSnapshot {
    addr: u32,
    bytes: Vec<u8>,
}

#[derive(Serialize)]
struct ResourceBusEvent {
    tick: u64,
    direction: &'static str,
    addr: u32,
    size: &'static str,
    value: Option<u32>,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    sr: u16,
    current_entry: Option<TaskSummary>,
    registers: RegisterSnapshot,
    offset_from_resource: i32,
}

#[derive(Serialize)]
struct ResourceChangeEvent {
    tick: u64,
    kind: &'static str,
    pc: u32,
    instr_start_pc: u32,
    current_entry: Option<TaskSummary>,
    registers: RegisterSnapshot,
    resource: ResourceSnapshot,
}

#[derive(Clone, Serialize, PartialEq, Eq)]
struct TaskSummary {
    addr: u32,
    name: Option<String>,
    state: u8,
    sig_wait: u32,
    sig_recvd: u32,
}

#[derive(Clone, Serialize, PartialEq, Eq)]
struct ReadyQueueSnapshot {
    head: u32,
    tail: u32,
    tail_pred: u32,
    head_task: Option<TaskSummary>,
}

#[derive(Clone, Copy, Serialize, PartialEq, Eq)]
struct DmacSnapshot {
    istr: u8,
    wd_asr: u8,
    wd_scsi_status: u8,
    wd_selected_reg: u8,
}

#[derive(Clone, Serialize, PartialEq, Eq)]
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

#[derive(Clone, Serialize, PartialEq, Eq)]
struct WaitNodeSnapshot {
    addr: u32,
    succ: u32,
    pred: u32,
    node_type: u8,
    node_pri: u8,
    node_name: u32,
    node_name_task: Option<TaskSummary>,
}

#[derive(Clone, Serialize, PartialEq, Eq)]
struct WaitListSnapshot {
    addr: u32,
    head: u32,
    tail: u32,
    tail_pred: u32,
    head_node: Option<WaitNodeSnapshot>,
}

#[derive(Clone, Serialize, PartialEq, Eq)]
struct OwnerSnapshot {
    addr: u32,
    queue_head: u32,
    queue_tail: u32,
    status_word: u16,
    count_word: u16,
    wait_list_head: u32,
    wait_list_tail: u32,
    wait_list_tail_pred: u32,
    resource_ptr: u32,
    resource_cache: u32,
    exec_base_ptr: u32,
    helper_arg_bytes: [u8; OWNER_HELPER_ARG_BYTES],
    entry_a0_addr: Option<u32>,
    entry_a0: Option<SemaphoreSnapshot>,
}

#[derive(Clone, Serialize, PartialEq, Eq)]
struct SemaphoreSnapshot {
    addr: u32,
    node_type: u8,
    name_ptr: u32,
    name: Option<String>,
    nest_count: i16,
    wait_list_head: u32,
    wait_list_tail: u32,
    wait_list_tail_pred: u32,
    multiple_link: u32,
    owner_ptr: u32,
    owner_task: Option<TaskSummary>,
    queue_count: i16,
}

#[derive(Clone, Serialize, PartialEq, Eq)]
struct RawObjectSnapshot {
    addr: u32,
    bytes: [u8; PRODUCER_OBJECT_BYTES],
}

#[derive(Clone, Serialize, PartialEq, Eq)]
struct ProducerSnapshot {
    addr: u32,
    source_ptr: u32,
    source_pending_ptr: Option<u32>,
    selected_ptr: u32,
    pending_ptr: u32,
    screen_ptr: u32,
    current_ptr: u32,
    fallback_ptr: u32,
    byte0: u8,
    byte1: u8,
    word0: u16,
    word1: u16,
    local_result_ptr: Option<u32>,
    source_obj: Option<RawObjectSnapshot>,
    source_pending_obj: Option<RawObjectSnapshot>,
    pending_obj: Option<RawObjectSnapshot>,
    screen_obj: Option<RawObjectSnapshot>,
    current_obj: Option<RawObjectSnapshot>,
    fallback_obj: Option<RawObjectSnapshot>,
    local_result_obj: Option<RawObjectSnapshot>,
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
    let data = serde_json::to_vec_pretty(report).expect("serialize late wait trace report");
    fs::write(&path, data).expect("write late wait trace report");
    println!("late wait trace saved to {}", path.display());
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

fn sample_task_name(amiga: &Amiga, task: u32) -> Option<String> {
    read_c_string(
        amiga,
        read_bus_long(amiga, task + TASK_NAME_PTR_OFFSET),
        TASK_NAME_MAX_LEN,
    )
}

fn sample_task(amiga: &Amiga, task: u32) -> Option<TaskSummary> {
    if task == 0 || !is_ram_addr(amiga, task) {
        return None;
    }

    Some(TaskSummary {
        addr: task,
        name: sample_task_name(amiga, task),
        state: read_bus_byte(amiga, task + TASK_STATE_OFFSET),
        sig_wait: read_bus_long(amiga, task + TASK_SIG_WAIT_OFFSET),
        sig_recvd: read_bus_long(amiga, task + TASK_SIG_RECVD_OFFSET),
    })
}

fn sample_exec_state_sig(amiga: &Amiga, task: u32) -> ExecStateSig {
    ExecStateSig {
        state: read_bus_byte(amiga, task + TASK_STATE_OFFSET),
        sig_wait: read_bus_long(amiga, task + TASK_SIG_WAIT_OFFSET),
        sig_recvd: read_bus_long(amiga, task + TASK_SIG_RECVD_OFFSET),
    }
}

fn sample_ready_queue_sig(amiga: &Amiga, exec_base: u32) -> ReadyQueueSig {
    ReadyQueueSig {
        head: read_bus_long(amiga, exec_base + 0x196),
        tail: read_bus_long(amiga, exec_base + 0x19A),
        tail_pred: read_bus_long(amiga, exec_base + 0x19E),
    }
}

fn sample_ready_queue(amiga: &Amiga, exec_base: u32) -> ReadyQueueSnapshot {
    let sig = sample_ready_queue_sig(amiga, exec_base);
    let empty_sentinel = exec_base + 0x19A;
    let head_task = if sig.head == 0 || sig.head == empty_sentinel {
        None
    } else {
        sample_task(amiga, sig.head)
    };

    ReadyQueueSnapshot {
        head: sig.head,
        tail: sig.tail,
        tail_pred: sig.tail_pred,
        head_task,
    }
}

fn sample_wait_node_sig(amiga: &Amiga, node_addr: u32) -> Option<WaitNodeSig> {
    if node_addr == 0 || !is_ram_addr(amiga, node_addr) {
        return None;
    }

    Some(WaitNodeSig {
        succ: read_bus_long(amiga, node_addr + NODE_SUCC_OFFSET),
        pred: read_bus_long(amiga, node_addr + NODE_PRED_OFFSET),
        node_type: read_bus_byte(amiga, node_addr + NODE_TYPE_OFFSET),
        node_pri: read_bus_byte(amiga, node_addr + NODE_PRI_OFFSET),
        node_name: read_bus_long(amiga, node_addr + NODE_NAME_OFFSET),
    })
}

fn sample_wait_node(amiga: &Amiga, node_addr: u32) -> Option<WaitNodeSnapshot> {
    let sig = sample_wait_node_sig(amiga, node_addr)?;
    let node_name_task = if sig.node_name != 0 && is_ram_addr(amiga, sig.node_name) {
        sample_task(amiga, sig.node_name)
    } else {
        None
    };

    Some(WaitNodeSnapshot {
        addr: node_addr,
        succ: sig.succ,
        pred: sig.pred,
        node_type: sig.node_type,
        node_pri: sig.node_pri,
        node_name: sig.node_name,
        node_name_task,
    })
}

fn sample_wait_list_sig(amiga: &Amiga, list_addr: u32) -> Option<WaitListSig> {
    if list_addr == 0 || !is_ram_addr(amiga, list_addr) {
        return None;
    }

    Some(WaitListSig {
        head: read_bus_long(amiga, list_addr + LIST_HEAD_OFFSET),
        tail: read_bus_long(amiga, list_addr + LIST_TAIL_OFFSET),
        tail_pred: read_bus_long(amiga, list_addr + LIST_TAIL_PRED_OFFSET),
    })
}

fn sample_wait_list(amiga: &Amiga, list_addr: u32) -> Option<WaitListSnapshot> {
    let sig = sample_wait_list_sig(amiga, list_addr)?;
    let empty_list_sentinel = list_addr + LIST_TAIL_OFFSET;
    let head_node = if sig.head == 0 || sig.head == empty_list_sentinel {
        None
    } else {
        sample_wait_node(amiga, sig.head)
    };

    Some(WaitListSnapshot {
        addr: list_addr,
        head: sig.head,
        tail: sig.tail,
        tail_pred: sig.tail_pred,
        head_node,
    })
}

fn read_bytes<const N: usize>(amiga: &Amiga, addr: u32) -> [u8; N] {
    let mut bytes = [0u8; N];
    for (offset, byte) in bytes.iter_mut().enumerate() {
        *byte = read_bus_byte(amiga, addr.wrapping_add(offset as u32));
    }
    bytes
}

fn sample_raw_object(amiga: &Amiga, addr: u32) -> Option<RawObjectSnapshot> {
    if addr == 0 || !is_ram_addr(amiga, addr) {
        return None;
    }

    Some(RawObjectSnapshot {
        addr,
        bytes: read_bytes::<PRODUCER_OBJECT_BYTES>(amiga, addr),
    })
}

fn sample_handoff(amiga: &Amiga, addr: u32) -> Option<HandoffSnapshot> {
    if addr == 0 || !is_ram_addr(amiga, addr) {
        return None;
    }

    Some(HandoffSnapshot {
        addr,
        bytes: read_bytes::<HANDOFF_OBJECT_BYTES>(amiga, addr).to_vec(),
    })
}

fn sample_resource(amiga: &Amiga, addr: u32) -> Option<ResourceSnapshot> {
    if addr == 0 || !is_ram_addr(amiga, addr) {
        return None;
    }

    Some(ResourceSnapshot {
        addr,
        bytes: read_bytes::<RESOURCE_OBJECT_BYTES>(amiga, addr).to_vec(),
    })
}

fn sample_semaphore(amiga: &Amiga, addr: u32) -> Option<SemaphoreSnapshot> {
    if addr == 0 || !is_ram_addr(amiga, addr) {
        return None;
    }

    let wait_list_addr = addr + SEMAPHORE_WAIT_LIST_OFFSET;
    let wait_list = sample_wait_list_sig(amiga, wait_list_addr)?;
    let owner_ptr = read_bus_long(amiga, addr + SEMAPHORE_OWNER_OFFSET);
    let owner_task = if is_ram_addr(amiga, owner_ptr) {
        sample_task(amiga, owner_ptr)
    } else {
        None
    };
    let name_ptr = read_bus_long(amiga, addr + NODE_NAME_OFFSET);

    Some(SemaphoreSnapshot {
        addr,
        node_type: read_bus_byte(amiga, addr + NODE_TYPE_OFFSET),
        name_ptr,
        name: read_c_string(amiga, name_ptr, TASK_NAME_MAX_LEN),
        nest_count: read_bus_i16(amiga, addr + SEMAPHORE_NEST_COUNT_OFFSET),
        wait_list_head: wait_list.head,
        wait_list_tail: wait_list.tail,
        wait_list_tail_pred: wait_list.tail_pred,
        multiple_link: read_bus_long(amiga, addr + SEMAPHORE_MULTIPLE_LINK_OFFSET),
        owner_ptr,
        owner_task,
        queue_count: read_bus_i16(amiga, addr + SEMAPHORE_QUEUE_COUNT_OFFSET),
    })
}

fn sample_owner(
    amiga: &Amiga,
    owner_addr: u32,
    entry_a0_addr: Option<u32>,
) -> Option<OwnerSnapshot> {
    if owner_addr == 0 || !is_ram_addr(amiga, owner_addr) {
        return None;
    }

    let wait_list_addr = owner_addr + OWNER_WAIT_LIST_OFFSET;
    let wait_list = sample_wait_list_sig(amiga, wait_list_addr)?;
    let entry_a0 = entry_a0_addr.and_then(|addr| sample_semaphore(amiga, addr));

    Some(OwnerSnapshot {
        addr: owner_addr,
        queue_head: read_bus_long(amiga, owner_addr + OWNER_QUEUE_HEAD_OFFSET),
        queue_tail: read_bus_long(amiga, owner_addr + OWNER_QUEUE_TAIL_OFFSET),
        status_word: read_bus_word(amiga, owner_addr + OWNER_STATUS_OFFSET),
        count_word: read_bus_word(amiga, owner_addr + OWNER_COUNT_OFFSET),
        wait_list_head: wait_list.head,
        wait_list_tail: wait_list.tail,
        wait_list_tail_pred: wait_list.tail_pred,
        resource_ptr: read_bus_long(amiga, owner_addr + OWNER_RESOURCE_OFFSET),
        resource_cache: read_bus_long(amiga, owner_addr + OWNER_RESOURCE_CACHE_OFFSET),
        exec_base_ptr: read_bus_long(amiga, owner_addr + OWNER_EXECBASE_PTR_OFFSET),
        helper_arg_bytes: read_bytes::<OWNER_HELPER_ARG_BYTES>(
            amiga,
            owner_addr + OWNER_HELPER_ARG_OFFSET,
        ),
        entry_a0_addr,
        entry_a0,
    })
}

fn sample_producer(amiga: &Amiga, addr: u32) -> Option<ProducerSnapshot> {
    if addr == 0 || !is_ram_addr(amiga, addr) {
        return None;
    }

    let source_ptr = read_bus_long(amiga, addr + PRODUCER_SOURCE_PTR_OFFSET);
    let screen_ptr = read_bus_long(amiga, addr + PRODUCER_SCREEN_PTR_OFFSET);
    let current_ptr = read_bus_long(amiga, addr + PRODUCER_CURRENT_PTR_OFFSET);
    let fallback_ptr = read_bus_long(amiga, addr + PRODUCER_FALLBACK_PTR_OFFSET);
    let source_pending_ptr = if source_ptr != 0 && is_ram_addr(amiga, source_ptr) {
        Some(read_bus_long(
            amiga,
            source_ptr + PRODUCER_SOURCE_PENDING_PTR_OFFSET,
        ))
    } else {
        None
    };
    let local_result_ptr =
        if (PRODUCER_ENTRY_PC..=PRODUCER_OUTER_END_PC).contains(&amiga.cpu.instr_start_pc) {
            let local_addr = amiga.cpu.regs.a[5].wrapping_sub(4);
            if is_ram_addr(amiga, local_addr) {
                Some(read_bus_long(amiga, local_addr))
            } else {
                None
            }
        } else {
            None
        };
    let pending_ptr = read_bus_long(amiga, addr + PRODUCER_PENDING_PTR_OFFSET);

    Some(ProducerSnapshot {
        addr,
        source_ptr,
        source_pending_ptr,
        selected_ptr: read_bus_long(amiga, addr + PRODUCER_SELECTED_PTR_OFFSET),
        pending_ptr,
        screen_ptr,
        current_ptr,
        fallback_ptr,
        byte0: read_bus_byte(amiga, addr + PRODUCER_BYTE0_OFFSET),
        byte1: read_bus_byte(amiga, addr + PRODUCER_BYTE1_OFFSET),
        word0: read_bus_word(amiga, addr + PRODUCER_WORD0_OFFSET),
        word1: read_bus_word(amiga, addr + PRODUCER_WORD1_OFFSET),
        local_result_ptr,
        source_obj: sample_raw_object(amiga, source_ptr),
        source_pending_obj: source_pending_ptr.and_then(|ptr| sample_raw_object(amiga, ptr)),
        pending_obj: sample_raw_object(amiga, pending_ptr),
        screen_obj: sample_raw_object(amiga, screen_ptr),
        current_obj: sample_raw_object(amiga, current_ptr),
        fallback_obj: sample_raw_object(amiga, fallback_ptr),
        local_result_obj: local_result_ptr.and_then(|ptr| sample_raw_object(amiga, ptr)),
    })
}

fn sample_dmac(amiga: &Amiga) -> DmacSnapshot {
    let dmac = amiga.dmac.as_ref().expect("A3000 should expose SDMAC");
    DmacSnapshot {
        istr: dmac.current_istr(),
        wd_asr: dmac.wd_asr(),
        wd_scsi_status: dmac.wd_scsi_status(),
        wd_selected_reg: dmac.wd_selected_reg(),
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

fn bus_write_value(addr: u32, is_word: bool, data: u16) -> u32 {
    if is_word {
        u32::from(data)
    } else if addr & 1 == 0 {
        u32::from(data >> 8)
    } else {
        u32::from(data & 0x00FF)
    }
}

fn wait_write_is_relevant(addr: u32, value: u32, targets: WaitTargets) -> bool {
    let wait_list_end = targets.wait_list_addr + 0x0C;
    let wait_entry_end = targets.wait_entry_addr + 0x10;
    let list_tail_sentinel = targets.wait_list_addr + LIST_TAIL_OFFSET;

    (targets.wait_list_addr..wait_list_end).contains(&addr)
        || (targets.wait_entry_addr..wait_entry_end).contains(&addr)
        || value == targets.wait_list_addr
        || value == list_tail_sentinel
        || value == targets.wait_entry_addr
}

fn owner_write_target(addr: u32, owner_addr: u32, entry_a0_addr: u32) -> Option<&'static str> {
    let owner_queue_end = owner_addr + OWNER_QUEUE_TAIL_OFFSET + 4;
    let owner_status_end = owner_addr + OWNER_COUNT_OFFSET + 2;
    let owner_wait_list_end = owner_addr + OWNER_WAIT_LIST_OFFSET + 0x0C;
    let owner_resource_end = owner_addr + OWNER_RESOURCE_CACHE_OFFSET + 4;
    let owner_execbase_end = owner_addr + OWNER_EXECBASE_PTR_OFFSET + 4;
    let owner_helper_arg_end = owner_addr + OWNER_HELPER_ARG_OFFSET + OWNER_HELPER_ARG_BYTES as u32;
    let semaphore_wait_list_end = entry_a0_addr + SEMAPHORE_WAIT_LIST_OFFSET + 0x0C;
    let semaphore_owner_end = entry_a0_addr + SEMAPHORE_QUEUE_COUNT_OFFSET + 2;

    if (owner_addr + OWNER_QUEUE_HEAD_OFFSET..owner_queue_end).contains(&addr) {
        Some("owner_queue")
    } else if (owner_addr + OWNER_STATUS_OFFSET..owner_status_end).contains(&addr) {
        Some("owner_status")
    } else if (owner_addr + OWNER_WAIT_LIST_OFFSET..owner_wait_list_end).contains(&addr) {
        Some("owner_wait_list")
    } else if (owner_addr + OWNER_RESOURCE_OFFSET..owner_resource_end).contains(&addr) {
        Some("owner_resource")
    } else if (owner_addr + OWNER_EXECBASE_PTR_OFFSET..owner_execbase_end).contains(&addr) {
        Some("owner_execbase")
    } else if (owner_addr + OWNER_HELPER_ARG_OFFSET..owner_helper_arg_end).contains(&addr) {
        Some("owner_helper_arg")
    } else if (entry_a0_addr + SEMAPHORE_NEST_COUNT_OFFSET
        ..entry_a0_addr + SEMAPHORE_NEST_COUNT_OFFSET + 2)
        .contains(&addr)
    {
        Some("entry_a0_nest_count")
    } else if (entry_a0_addr + SEMAPHORE_WAIT_LIST_OFFSET..semaphore_wait_list_end).contains(&addr)
    {
        Some("entry_a0_wait_list")
    } else if (entry_a0_addr + SEMAPHORE_MULTIPLE_LINK_OFFSET
        ..entry_a0_addr + SEMAPHORE_MULTIPLE_LINK_OFFSET + 4)
        .contains(&addr)
    {
        Some("entry_a0_multiple_link")
    } else if (entry_a0_addr + SEMAPHORE_OWNER_OFFSET..semaphore_owner_end).contains(&addr) {
        Some("entry_a0_owner_or_queue")
    } else {
        None
    }
}

fn producer_pc_is_relevant(pc: u32) -> bool {
    matches!(
        pc,
        PRODUCER_ENTRY_PC
            | PRODUCER_WAIT_WRAPPER_PC
            | PRODUCER_FREE_PREV_PC
            | PRODUCER_STORE_PENDING_PC
            | PRODUCER_RELEASE_SLOT4_PC
    )
}

fn producer_bus_pc_is_relevant(pc: u32) -> bool {
    (PRODUCER_TRACE_START_PC..=PRODUCER_TRACE_END_PC).contains(&pc)
}

fn relative_offset(base: u32, addr: u32) -> Option<i32> {
    let diff = i64::from(addr) - i64::from(base);
    if (-0x4000..=0x4000).contains(&diff) {
        Some(diff as i32)
    } else {
        None
    }
}

fn make_event(
    amiga: &Amiga,
    tick: u64,
    kind: &'static str,
    exec_base: u32,
    exec_task: u32,
    current_entry_addr: u32,
    wait_node_addr: Option<u32>,
    wait_list_addr: Option<u32>,
    owner_addr: Option<u32>,
    entry_a0_addr: Option<u32>,
    producer_addr: Option<u32>,
) -> TraceEvent {
    TraceEvent {
        tick,
        kind,
        pc: amiga.cpu.regs.pc,
        instr_start_pc: amiga.cpu.instr_start_pc,
        ir: amiga.cpu.ir,
        sr: amiga.cpu.regs.sr,
        intena: amiga.paula.intena,
        intreq: amiga.paula.intreq,
        registers: sample_registers(amiga),
        stack_chain: stack_chain(amiga),
        current_entry: sample_task(amiga, current_entry_addr),
        exec_task: sample_task(amiga, exec_task).expect("exec.library task should stay valid"),
        ready_queue: sample_ready_queue(amiga, exec_base),
        dmac: sample_dmac(amiga),
        wait_node_addr,
        wait_list_addr,
        wait_node: wait_node_addr.and_then(|addr| sample_wait_node(amiga, addr)),
        wait_list: wait_list_addr.and_then(|addr| sample_wait_list(amiga, addr)),
        owner_addr,
        owner: owner_addr.and_then(|addr| sample_owner(amiga, addr, entry_a0_addr)),
        producer_addr,
        producer: producer_addr.and_then(|addr| sample_producer(amiga, addr)),
    }
}

fn discover_wait_targets(window_start_tick: u64, window_end_tick: u64) -> WaitTargets {
    let mut amiga = build_amiga().expect("load Kickstart ROM for wait target discovery");
    let mut tracked_wait_list_addr = None;

    for tick in 0..=window_end_tick {
        amiga.tick();

        if tick < window_start_tick {
            continue;
        }

        if (WAIT_NODE_HELPER_START..=WAIT_NODE_HELPER_END).contains(&amiga.cpu.instr_start_pc)
            || amiga.cpu.instr_start_pc == WAIT_NODE_SETUP_PC
        {
            let wait_list_addr = amiga.cpu.regs.a(5);
            if is_ram_addr(&amiga, wait_list_addr) {
                tracked_wait_list_addr = Some(wait_list_addr);
            }
        }

        if amiga.cpu.regs.pc == STOP_RESUME_PC && amiga.cpu.ir == 0x4E72 {
            let wait_list_addr =
                tracked_wait_list_addr.expect("late wait target discovery should find a list");
            let wait_list = sample_wait_list_sig(&amiga, wait_list_addr)
                .expect("late wait target discovery should sample the list");
            let wait_entry_addr = wait_list.head;
            let empty_list_sentinel = wait_list_addr + LIST_TAIL_OFFSET;
            assert_ne!(
                wait_entry_addr, empty_list_sentinel,
                "late wait target discovery should stop with a queued waiter"
            );
            return WaitTargets {
                wait_list_addr,
                wait_entry_addr,
            };
        }
    }

    panic!("late wait target discovery did not reach the STOP loop");
}

fn discover_owner_context(window_end_tick: u64) -> OwnerContext {
    let mut amiga = build_amiga().expect("load Kickstart ROM for owner discovery");

    for tick in 0..=window_end_tick {
        amiga.tick();
        if amiga.cpu.instr_start_pc == WAIT_NODE_HELPER_START {
            let owner_addr = amiga.cpu.regs.a(6);
            let entry_a0_addr = amiga.cpu.regs.a(0);
            assert!(
                is_ram_addr(&amiga, owner_addr),
                "late wait owner discovery should find a RAM-backed owner frame"
            );
            assert!(
                is_ram_addr(&amiga, entry_a0_addr),
                "late wait owner discovery should find a RAM-backed A0 payload"
            );
            return OwnerContext {
                helper_start_tick: tick,
                owner_addr,
                entry_a0_addr,
            };
        }
    }

    panic!("late wait owner discovery did not reach the helper");
}

fn discover_producer_context(helper_start_tick: u64) -> ProducerContext {
    let mut amiga = build_amiga().expect("load Kickstart ROM for producer discovery");
    let mut producer_context = None;

    for _tick in 0..=helper_start_tick {
        amiga.tick();
        if producer_bus_pc_is_relevant(amiga.cpu.instr_start_pc) {
            let base_addr = amiga.cpu.regs.a[6];
            assert!(
                is_ram_addr(&amiga, base_addr),
                "late wait producer discovery should find a RAM-backed producer base"
            );
            producer_context = Some(ProducerContext { base_addr });
        }
    }

    producer_context.expect("late wait producer discovery should find the producer path")
}

fn discover_handoff_context(helper_start_tick: u64, window_start_tick: u64) -> HandoffContext {
    let mut amiga = build_amiga().expect("load Kickstart ROM for handoff discovery");
    let mut handoff_context = None;

    for tick in 0..=helper_start_tick {
        amiga.tick();
        if tick < window_start_tick {
            continue;
        }

        if producer_bus_pc_is_relevant(amiga.cpu.instr_start_pc) {
            let handoff_addr = amiga.cpu.regs.a[1];
            let source_addr = amiga.cpu.regs.a[0];
            if is_ram_addr(&amiga, handoff_addr) && is_ram_addr(&amiga, source_addr) {
                handoff_context = Some(HandoffContext {
                    trace_start_tick: tick.saturating_sub(PRE_HANDOFF_TRACE_TICKS),
                    addr: handoff_addr,
                    source_addr,
                });
            }
        }
    }

    handoff_context.expect("late wait handoff discovery should find the final object handoff")
}

fn discover_resource_context(
    helper_start_tick: u64,
    trace_start_tick: u64,
    owner_addr: u32,
) -> ResourceContext {
    let mut amiga = build_amiga().expect("load Kickstart ROM for resource discovery");
    let mut resource_context = None;

    for tick in 0..=helper_start_tick {
        amiga.tick();
        if tick < trace_start_tick {
            continue;
        }

        let addr = read_bus_long(&amiga, owner_addr + OWNER_RESOURCE_OFFSET);
        if is_ram_addr(&amiga, addr) {
            resource_context = Some(ResourceContext {
                trace_start_tick: tick.saturating_sub(PRE_RESOURCE_TRACE_TICKS),
                addr,
            });
        }
    }

    resource_context.expect("late wait resource discovery should find the owner resource")
}

fn discover_stop_context() -> StopContext {
    let mut amiga = build_amiga().expect("load Kickstart ROM for stop discovery");

    for tick in 0..MAX_BOOT_TICKS {
        amiga.tick();
        if amiga.cpu.regs.pc == STOP_RESUME_PC && amiga.cpu.ir == 0x4E72 {
            return StopContext {
                stop_tick: tick,
                exec_base: amiga.cpu.regs.a[6],
                exec_task: amiga.cpu.regs.a[1],
            };
        }
    }

    panic!("did not reach STOP loop while discovering late wait trace context");
}

fn discover_signal_window(exec_task: u32, stop_tick: u64) -> SignalWindow {
    let mut amiga = build_amiga().expect("load Kickstart ROM for signal window discovery");
    let mut prev_sig_recvd = read_bus_long(&amiga, exec_task + TASK_SIG_RECVD_OFFSET);
    let mut last_signal_set_tick = None;
    let mut last_signal_clear_tick = None;

    for tick in 0..=stop_tick {
        amiga.tick();
        let current_sig_recvd = read_bus_long(&amiga, exec_task + TASK_SIG_RECVD_OFFSET);
        if current_sig_recvd != prev_sig_recvd {
            if (current_sig_recvd & 0x10) != 0 {
                last_signal_set_tick = Some(tick);
            }
            if (prev_sig_recvd & 0x10) != 0 && (current_sig_recvd & 0x10) == 0 {
                last_signal_clear_tick = Some(tick);
            }
            prev_sig_recvd = current_sig_recvd;
        }
    }

    SignalWindow {
        last_signal_set_tick: last_signal_set_tick.expect("late signal trace should observe a set"),
        last_signal_clear_tick: last_signal_clear_tick
            .expect("late signal trace should observe a matching clear"),
    }
}

fn run_late_wait_trace() {
    let context = discover_stop_context();
    let signal_window = discover_signal_window(context.exec_task, context.stop_tick);
    let window_start_tick = signal_window
        .last_signal_set_tick
        .saturating_sub(PRE_LAST_SIGNAL_TICKS);
    let window_end_tick = context.stop_tick.saturating_add(POST_STOP_TRACE_TICKS);
    let owner_context = discover_owner_context(window_end_tick);
    let producer_context = discover_producer_context(owner_context.helper_start_tick);
    let handoff_context =
        discover_handoff_context(owner_context.helper_start_tick, window_start_tick);
    let resource_context = discover_resource_context(
        owner_context.helper_start_tick,
        window_start_tick,
        owner_context.owner_addr,
    );
    let owner_trace_start_tick = owner_context
        .helper_start_tick
        .saturating_sub(PRE_HELPER_OWNER_TRACE_TICKS)
        .max(window_start_tick);
    let wait_targets = discover_wait_targets(window_start_tick, window_end_tick);

    let mut amiga = build_amiga().expect("load Kickstart ROM for late wait trace");
    let mut report = TraceReport {
        rom_path: "../../roms/kick31_40_068_a3000.rom",
        stop_tick: context.stop_tick,
        helper_entry_tick: owner_context.helper_start_tick,
        exec_base: context.exec_base,
        exec_task: context.exec_task,
        exec_task_name: None,
        last_signal_set_tick: signal_window.last_signal_set_tick,
        last_signal_clear_tick: signal_window.last_signal_clear_tick,
        window_start_tick,
        owner_trace_start_tick,
        final_wait_node_addr: None,
        final_wait_list_addr: Some(wait_targets.wait_list_addr),
        final_wait_entry_addr: Some(wait_targets.wait_entry_addr),
        final_owner_addr: Some(owner_context.owner_addr),
        final_entry_a0_addr: Some(owner_context.entry_a0_addr),
        final_producer_addr: Some(producer_context.base_addr),
        handoff_trace_start_tick: Some(handoff_context.trace_start_tick),
        final_handoff_addr: Some(handoff_context.addr),
        final_handoff_source_addr: Some(handoff_context.source_addr),
        final_handoff: None,
        resource_trace_start_tick: Some(resource_context.trace_start_tick),
        final_resource_addr: Some(resource_context.addr),
        final_resource: None,
        events: Vec::new(),
        bus_write_events: Vec::new(),
        owner_write_events: Vec::new(),
        producer_bus_events: Vec::new(),
        handoff_bus_events: Vec::new(),
        handoff_change_events: Vec::new(),
        resource_bus_events: Vec::new(),
        resource_change_events: Vec::new(),
        pc_samples: Vec::new(),
    };

    let mut prev_current_entry_addr = 0u32;
    let mut prev_exec_sig = sample_exec_state_sig(&amiga, context.exec_task);
    let mut prev_ready_queue_sig = sample_ready_queue_sig(&amiga, context.exec_base);
    let mut tracked_wait_node_addr = None;
    let mut tracked_wait_list_addr = None;
    let tracked_owner_addr = Some(owner_context.owner_addr);
    let tracked_entry_a0_addr = Some(owner_context.entry_a0_addr);
    let tracked_producer_addr = Some(producer_context.base_addr);
    let mut prev_wait_node_addr = None;
    let mut prev_wait_list_addr = None;
    let mut prev_wait_node_sig = None;
    let mut prev_wait_list_sig = None;
    let mut prev_owner_snapshot =
        tracked_owner_addr.and_then(|addr| sample_owner(&amiga, addr, tracked_entry_a0_addr));
    let mut prev_producer_snapshot =
        tracked_producer_addr.and_then(|addr| sample_producer(&amiga, addr));
    let tracked_handoff_addr = Some(handoff_context.addr);
    let mut prev_handoff_snapshot =
        tracked_handoff_addr.and_then(|addr| sample_handoff(&amiga, addr));
    let tracked_resource_addr = Some(resource_context.addr);
    let mut prev_resource_snapshot = None;
    let mut prev_bus_sig: Option<(u32, u8, bool, bool, Option<u16>)> = None;
    let mut last_relevant_producer_pc = None;
    let mut stop_recorded = false;

    for tick in 0..=window_end_tick {
        amiga.tick();

        if tick < window_start_tick {
            continue;
        }

        let current_entry_addr = read_bus_long(&amiga, context.exec_base + 0x114);
        let exec_sig = sample_exec_state_sig(&amiga, context.exec_task);
        let ready_queue_sig = sample_ready_queue_sig(&amiga, context.exec_base);
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

        if (WAIT_NODE_HELPER_START..=WAIT_NODE_HELPER_END).contains(&amiga.cpu.instr_start_pc)
            || amiga.cpu.instr_start_pc == WAIT_NODE_SETUP_PC
        {
            let wait_node_addr = amiga.cpu.regs.a(7);
            let wait_list_addr = amiga.cpu.regs.a(5);
            if is_ram_addr(&amiga, wait_node_addr) {
                tracked_wait_node_addr = Some(wait_node_addr);
            }
            if is_ram_addr(&amiga, wait_list_addr) {
                tracked_wait_list_addr = Some(wait_list_addr);
            }
        }

        let wait_node_sig =
            tracked_wait_node_addr.and_then(|addr| sample_wait_node_sig(&amiga, addr));
        let wait_list_sig =
            tracked_wait_list_addr.and_then(|addr| sample_wait_list_sig(&amiga, addr));
        let owner_snapshot =
            tracked_owner_addr.and_then(|addr| sample_owner(&amiga, addr, tracked_entry_a0_addr));
        let producer_snapshot =
            tracked_producer_addr.and_then(|addr| sample_producer(&amiga, addr));
        let handoff_snapshot = if tick >= handoff_context.trace_start_tick {
            tracked_handoff_addr.and_then(|addr| sample_handoff(&amiga, addr))
        } else {
            None
        };
        let resource_snapshot = if tick >= resource_context.trace_start_tick {
            tracked_resource_addr.and_then(|addr| sample_resource(&amiga, addr))
        } else {
            None
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

                if tracked_wait_list_addr.is_some()
                    && !is_read
                    && fc != FunctionCode::InterruptAck
                    && report.bus_write_events.len() < MAX_BUS_WRITE_EVENTS
                {
                    let value = bus_write_value(
                        addr,
                        is_word,
                        data.expect("write bus cycle should carry data"),
                    );
                    if wait_write_is_relevant(addr, value, wait_targets) {
                        report.bus_write_events.push(BusWriteEvent {
                            tick,
                            addr,
                            size: if is_word { "word" } else { "byte" },
                            value,
                            pc: amiga.cpu.regs.pc,
                            instr_start_pc: amiga.cpu.instr_start_pc,
                            ir: amiga.cpu.ir,
                            sr: amiga.cpu.regs.sr,
                            current_entry: sample_task(&amiga, current_entry_addr),
                            registers: sample_registers(&amiga),
                        });
                    }
                }

                if tick >= owner_trace_start_tick
                    && tick <= owner_context.helper_start_tick
                    && !is_read
                    && fc != FunctionCode::InterruptAck
                    && report.owner_write_events.len() < MAX_OWNER_WRITE_EVENTS
                {
                    let value = bus_write_value(
                        addr,
                        is_word,
                        data.expect("write bus cycle should carry data"),
                    );
                    if let Some(target) = owner_write_target(
                        addr,
                        owner_context.owner_addr,
                        owner_context.entry_a0_addr,
                    ) {
                        report.owner_write_events.push(OwnerWriteEvent {
                            tick,
                            target,
                            addr,
                            size: if is_word { "word" } else { "byte" },
                            value,
                            pc: amiga.cpu.regs.pc,
                            instr_start_pc: amiga.cpu.instr_start_pc,
                            ir: amiga.cpu.ir,
                            sr: amiga.cpu.regs.sr,
                            current_entry: sample_task(&amiga, current_entry_addr),
                            registers: sample_registers(&amiga),
                            owner: sample_owner(
                                &amiga,
                                owner_context.owner_addr,
                                Some(owner_context.entry_a0_addr),
                            )
                            .expect("owner write trace should sample the owner"),
                        });
                    }
                }

                if producer_bus_pc_is_relevant(amiga.cpu.instr_start_pc)
                    && fc != FunctionCode::InterruptAck
                    && report.producer_bus_events.len() < MAX_PRODUCER_BUS_EVENTS
                {
                    let registers = sample_registers(&amiga);
                    report.producer_bus_events.push(ProducerBusEvent {
                        tick,
                        direction: if is_read { "read" } else { "write" },
                        addr,
                        size: if is_word { "word" } else { "byte" },
                        value: data.map(|raw| bus_write_value(addr, is_word, raw)),
                        pc: amiga.cpu.regs.pc,
                        instr_start_pc: amiga.cpu.instr_start_pc,
                        ir: amiga.cpu.ir,
                        sr: amiga.cpu.regs.sr,
                        current_entry: sample_task(&amiga, current_entry_addr),
                        offset_from_a2: relative_offset(registers.a2, addr),
                        offset_from_a5: relative_offset(registers.a5, addr),
                        offset_from_a6: relative_offset(registers.a6, addr),
                        offset_from_producer: relative_offset(producer_context.base_addr, addr),
                        registers,
                    });
                }

                if tick >= handoff_context.trace_start_tick
                    && fc != FunctionCode::InterruptAck
                    && report.handoff_bus_events.len() < MAX_HANDOFF_BUS_EVENTS
                    && (handoff_context.addr..handoff_context.addr + HANDOFF_OBJECT_BYTES as u32)
                        .contains(&addr)
                {
                    report.handoff_bus_events.push(HandoffBusEvent {
                        tick,
                        direction: if is_read { "read" } else { "write" },
                        addr,
                        size: if is_word { "word" } else { "byte" },
                        value: data.map(|raw| bus_write_value(addr, is_word, raw)),
                        pc: amiga.cpu.regs.pc,
                        instr_start_pc: amiga.cpu.instr_start_pc,
                        ir: amiga.cpu.ir,
                        sr: amiga.cpu.regs.sr,
                        current_entry: sample_task(&amiga, current_entry_addr),
                        registers: sample_registers(&amiga),
                        offset_from_handoff: i32::try_from(addr - handoff_context.addr)
                            .expect("tracked handoff access should fit in i32"),
                    });
                }

                if tick >= resource_context.trace_start_tick
                    && fc != FunctionCode::InterruptAck
                    && report.resource_bus_events.len() < MAX_RESOURCE_BUS_EVENTS
                    && (resource_context.addr..resource_context.addr + RESOURCE_OBJECT_BYTES as u32)
                        .contains(&addr)
                {
                    report.resource_bus_events.push(ResourceBusEvent {
                        tick,
                        direction: if is_read { "read" } else { "write" },
                        addr,
                        size: if is_word { "word" } else { "byte" },
                        value: data.map(|raw| bus_write_value(addr, is_word, raw)),
                        pc: amiga.cpu.regs.pc,
                        instr_start_pc: amiga.cpu.instr_start_pc,
                        ir: amiga.cpu.ir,
                        sr: amiga.cpu.regs.sr,
                        current_entry: sample_task(&amiga, current_entry_addr),
                        registers: sample_registers(&amiga),
                        offset_from_resource: i32::try_from(addr - resource_context.addr)
                            .expect("tracked resource access should fit in i32"),
                    });
                }
            }
        }
        prev_bus_sig = current_bus_sig;

        if report.events.is_empty() {
            report.events.push(make_event(
                &amiga,
                tick,
                "window_start",
                context.exec_base,
                context.exec_task,
                current_entry_addr,
                tracked_wait_node_addr,
                tracked_wait_list_addr,
                tracked_owner_addr,
                tracked_entry_a0_addr,
                tracked_producer_addr,
            ));
        }

        if report.events.len() < MAX_EVENTS && current_entry_addr != prev_current_entry_addr {
            report.events.push(make_event(
                &amiga,
                tick,
                "current_entry_change",
                context.exec_base,
                context.exec_task,
                current_entry_addr,
                tracked_wait_node_addr,
                tracked_wait_list_addr,
                tracked_owner_addr,
                tracked_entry_a0_addr,
                tracked_producer_addr,
            ));
        }
        prev_current_entry_addr = current_entry_addr;

        if report.events.len() < MAX_EVENTS && exec_sig != prev_exec_sig {
            report.events.push(make_event(
                &amiga,
                tick,
                "exec_change",
                context.exec_base,
                context.exec_task,
                current_entry_addr,
                tracked_wait_node_addr,
                tracked_wait_list_addr,
                tracked_owner_addr,
                tracked_entry_a0_addr,
                tracked_producer_addr,
            ));
        }
        prev_exec_sig = exec_sig;

        if report.events.len() < MAX_EVENTS && ready_queue_sig != prev_ready_queue_sig {
            report.events.push(make_event(
                &amiga,
                tick,
                "ready_queue_change",
                context.exec_base,
                context.exec_task,
                current_entry_addr,
                tracked_wait_node_addr,
                tracked_wait_list_addr,
                tracked_owner_addr,
                tracked_entry_a0_addr,
                tracked_producer_addr,
            ));
        }
        prev_ready_queue_sig = ready_queue_sig;

        if report.events.len() < MAX_EVENTS && tracked_wait_node_addr != prev_wait_node_addr {
            report.events.push(make_event(
                &amiga,
                tick,
                "wait_node_addr_change",
                context.exec_base,
                context.exec_task,
                current_entry_addr,
                tracked_wait_node_addr,
                tracked_wait_list_addr,
                tracked_owner_addr,
                tracked_entry_a0_addr,
                tracked_producer_addr,
            ));
        }
        prev_wait_node_addr = tracked_wait_node_addr;

        if report.events.len() < MAX_EVENTS && tracked_wait_list_addr != prev_wait_list_addr {
            report.events.push(make_event(
                &amiga,
                tick,
                "wait_list_addr_change",
                context.exec_base,
                context.exec_task,
                current_entry_addr,
                tracked_wait_node_addr,
                tracked_wait_list_addr,
                tracked_owner_addr,
                tracked_entry_a0_addr,
                tracked_producer_addr,
            ));
        }
        prev_wait_list_addr = tracked_wait_list_addr;

        if report.events.len() < MAX_EVENTS && wait_node_sig != prev_wait_node_sig {
            report.events.push(make_event(
                &amiga,
                tick,
                "wait_node_change",
                context.exec_base,
                context.exec_task,
                current_entry_addr,
                tracked_wait_node_addr,
                tracked_wait_list_addr,
                tracked_owner_addr,
                tracked_entry_a0_addr,
                tracked_producer_addr,
            ));
        }
        prev_wait_node_sig = wait_node_sig;

        if report.events.len() < MAX_EVENTS && wait_list_sig != prev_wait_list_sig {
            report.events.push(make_event(
                &amiga,
                tick,
                "wait_list_change",
                context.exec_base,
                context.exec_task,
                current_entry_addr,
                tracked_wait_node_addr,
                tracked_wait_list_addr,
                tracked_owner_addr,
                tracked_entry_a0_addr,
                tracked_producer_addr,
            ));
        }
        prev_wait_list_sig = wait_list_sig;

        if report.events.len() < MAX_EVENTS && owner_snapshot != prev_owner_snapshot {
            report.events.push(make_event(
                &amiga,
                tick,
                "owner_change",
                context.exec_base,
                context.exec_task,
                current_entry_addr,
                tracked_wait_node_addr,
                tracked_wait_list_addr,
                tracked_owner_addr,
                tracked_entry_a0_addr,
                tracked_producer_addr,
            ));
        }
        prev_owner_snapshot = owner_snapshot;

        if report.events.len() < MAX_EVENTS && producer_snapshot != prev_producer_snapshot {
            report.events.push(make_event(
                &amiga,
                tick,
                "producer_change",
                context.exec_base,
                context.exec_task,
                current_entry_addr,
                tracked_wait_node_addr,
                tracked_wait_list_addr,
                tracked_owner_addr,
                tracked_entry_a0_addr,
                tracked_producer_addr,
            ));
        }
        prev_producer_snapshot = producer_snapshot;

        if tick >= handoff_context.trace_start_tick
            && report.handoff_change_events.len() < MAX_HANDOFF_CHANGE_EVENTS
            && handoff_snapshot != prev_handoff_snapshot
        {
            if let Some(handoff) = handoff_snapshot.clone() {
                report.handoff_change_events.push(HandoffChangeEvent {
                    tick,
                    kind: "handoff_change",
                    pc: amiga.cpu.regs.pc,
                    instr_start_pc: amiga.cpu.instr_start_pc,
                    current_entry: sample_task(&amiga, current_entry_addr),
                    registers: sample_registers(&amiga),
                    handoff,
                });
            }
        }
        prev_handoff_snapshot = handoff_snapshot;

        if tick >= resource_context.trace_start_tick
            && report.resource_change_events.len() < MAX_RESOURCE_CHANGE_EVENTS
            && resource_snapshot != prev_resource_snapshot
        {
            if let Some(resource) = resource_snapshot.clone() {
                report.resource_change_events.push(ResourceChangeEvent {
                    tick,
                    kind: "resource_change",
                    pc: amiga.cpu.regs.pc,
                    instr_start_pc: amiga.cpu.instr_start_pc,
                    current_entry: sample_task(&amiga, current_entry_addr),
                    registers: sample_registers(&amiga),
                    resource,
                });
            }
        }
        prev_resource_snapshot = resource_snapshot;

        if producer_pc_is_relevant(amiga.cpu.instr_start_pc) {
            if report.events.len() < MAX_EVENTS
                && last_relevant_producer_pc != Some(amiga.cpu.instr_start_pc)
            {
                report.events.push(make_event(
                    &amiga,
                    tick,
                    "producer_pc",
                    context.exec_base,
                    context.exec_task,
                    current_entry_addr,
                    tracked_wait_node_addr,
                    tracked_wait_list_addr,
                    tracked_owner_addr,
                    tracked_entry_a0_addr,
                    tracked_producer_addr,
                ));
            }
            last_relevant_producer_pc = Some(amiga.cpu.instr_start_pc);
        } else {
            last_relevant_producer_pc = None;
        }

        if !stop_recorded && amiga.cpu.regs.pc == STOP_RESUME_PC && amiga.cpu.ir == 0x4E72 {
            if report.events.len() < MAX_EVENTS {
                report.events.push(make_event(
                    &amiga,
                    tick,
                    "stop",
                    context.exec_base,
                    context.exec_task,
                    current_entry_addr,
                    tracked_wait_node_addr,
                    tracked_wait_list_addr,
                    tracked_owner_addr,
                    tracked_entry_a0_addr,
                    tracked_producer_addr,
                ));
            }
            stop_recorded = true;
        }

        if (tick - window_start_tick) % PC_SAMPLE_PERIOD == 0 {
            report.pc_samples.push(PcSample {
                tick,
                pc: amiga.cpu.regs.pc,
                instr_start_pc: amiga.cpu.instr_start_pc,
                ir: amiga.cpu.ir,
                current_entry: sample_task(&amiga, current_entry_addr).and_then(|task| task.name),
                exec_state: exec_sig.state,
                exec_sig_wait: exec_sig.sig_wait,
                exec_sig_recvd: exec_sig.sig_recvd,
                wait_node_addr: tracked_wait_node_addr,
                wait_list_addr: tracked_wait_list_addr,
                wait_list_head: wait_list_sig.map(|sig| sig.head),
            });
        }
    }

    report.exec_task_name = sample_task(&amiga, context.exec_task).and_then(|task| task.name);
    report.final_wait_node_addr = tracked_wait_node_addr;
    report.final_wait_list_addr = tracked_wait_list_addr;
    report.final_owner_addr = tracked_owner_addr;
    report.final_entry_a0_addr = tracked_entry_a0_addr;
    report.final_producer_addr = tracked_producer_addr;
    report.final_handoff_addr = tracked_handoff_addr;
    report.final_handoff_source_addr = Some(handoff_context.source_addr);
    report.final_handoff = tracked_handoff_addr.and_then(|addr| sample_handoff(&amiga, addr));
    report.final_resource_addr = tracked_resource_addr;
    report.final_resource = tracked_resource_addr.and_then(|addr| sample_resource(&amiga, addr));
    write_report("late_wait_trace_kick31_a3000", &report);
}

#[test]
#[ignore]
fn trace_last_signal_to_stop_a3000() {
    run_late_wait_trace();
}
