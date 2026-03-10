//! Focused post-SDMAC-pending IRQ6 trace for KS3.1 A3000.
//!
//! This test is ignored by default because it requires a local Kickstart ROM
//! and writes JSON under `test_output/amiga/traces/`.

mod common;

use std::fs;
use std::path::PathBuf;

use machine_amiga::mos_cia_8520::Cia8520;
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
const MAX_CUSTOM_WRITE_EVENTS: usize = 32;
const MAX_SERVER_CALLS: usize = 16;
const MAX_SERVER_NODES: usize = 8;
const MAX_CIAB_LVO_EVENTS: usize = 16;
const MAX_CIAB_CONTROL_EVENTS: usize = 64;
const MAX_CIAB_STATE_CHANGES: usize = 32;
const MAX_CIAB_HELPER_EVENTS: usize = 32;
const MAX_TASK_EVENTS: usize = 16;
const MAX_PC_SAMPLES: usize = 96;
const CIAB_RESOURCE_SLOT_COUNT: usize = 5;
const EXEC_BASE_ADDR: u32 = 0x0000_0004;
const EXEC_EXTER_DISPATCH_OFFSET: u32 = 0x00F0;
const TASK_ADDR: u32 = 132_133_496;

#[derive(Serialize)]
struct TraceReport {
    rom_path: &'static str,
    pending_tick: u64,
    pending_state: CpuSample,
    pending_dmac: DmacSnapshot,
    exec_base: u32,
    pending_cia_b: CiaSnapshot,
    exter_dispatch: InterruptDispatchSnapshot,
    ciab_resource: Option<CiabResourceSnapshot>,
    ciab_lvo_events: Vec<CiabLvoEvent>,
    ciab_control_events: Vec<CiabControlEvent>,
    ciab_state_changes: Vec<CiabStateChangeEvent>,
    ciab_helper_events: Vec<CiabHelperEvent>,
    stop_tick: Option<u64>,
    iack_events: Vec<IackEvent>,
    dmac_changes: Vec<DmacChange>,
    board_io_events: Vec<BoardIoEvent>,
    custom_write_events: Vec<CustomWriteEvent>,
    server_calls: Vec<ServerCallEvent>,
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

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
struct CiaSnapshot {
    timer_a: u16,
    timer_b: u16,
    icr_status: u8,
    icr_mask: u8,
    cra: u8,
    crb: u8,
    tod_counter: u32,
    tod_alarm: u32,
    tod_halted: bool,
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
    component: &'static str,
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
    dmac: DmacSnapshot,
}

#[derive(Serialize)]
struct ServerCallEvent {
    tick: u64,
    node_addr: u32,
    name: Option<String>,
    data: u32,
    code: u32,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    sr: u16,
    intena: u16,
    intreq: u16,
    dmac: DmacSnapshot,
}

#[derive(Serialize)]
struct InterruptDispatchSnapshot {
    list_ptr: u32,
    dispatcher: u32,
    ack_mask: u16,
    nodes: Vec<InterruptServerNode>,
}

#[derive(Serialize)]
struct InterruptServerNode {
    node_addr: u32,
    priority: i8,
    name: Option<String>,
    data: u32,
    code: u32,
}

#[derive(Serialize)]
struct CiabResourceSnapshot {
    base: u32,
    field_26: u16,
    field_28: u8,
    field_29: u8,
    field_84: u8,
    slots: Vec<CiabResourceSlot>,
}

#[derive(Serialize)]
struct CiabResourceSlot {
    bit: u8,
    slot_addr: u32,
    data: u32,
    code: u32,
    name: Option<String>,
}

#[derive(Serialize)]
struct CiabLvoEvent {
    tick: u64,
    lvo: &'static str,
    pc: u32,
    instr_start_pc: u32,
    d0: u32,
    a1: u32,
    a6: u32,
    interrupt_name: Option<String>,
}

#[derive(Serialize)]
struct CiabControlEvent {
    tick: u64,
    reg: u8,
    reg_name: &'static str,
    is_read: bool,
    value: Option<u8>,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    sr: u16,
    cia: CiaSnapshot,
    dmac: DmacSnapshot,
}

#[derive(Serialize)]
struct CiabStateChangeEvent {
    tick: u64,
    old: CiaSnapshot,
    new: CiaSnapshot,
    pc: u32,
    instr_start_pc: u32,
    ir: u16,
    sr: u16,
    bus_addr: Option<u32>,
    bus_is_read: Option<bool>,
    bus_value: Option<u8>,
    dmac: DmacSnapshot,
}

#[derive(Serialize)]
struct CiabHelperEvent {
    tick: u64,
    pc: u32,
    instr_start_pc: u32,
    stack_return_pc: u32,
    d0: u32,
    d1: u32,
    a0: u32,
    a1: u32,
    a6: u32,
    sr: u16,
    cia: CiaSnapshot,
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

fn sample_cia(cia: &Cia8520) -> CiaSnapshot {
    CiaSnapshot {
        timer_a: cia.timer_a(),
        timer_b: cia.timer_b(),
        icr_status: cia.icr_status(),
        icr_mask: cia.icr_mask(),
        cra: cia.cra(),
        crb: cia.crb(),
        tod_counter: cia.tod_counter(),
        tod_alarm: cia.tod_alarm(),
        tod_halted: cia.tod_halted(),
    }
}

fn sample_exter_dispatch(amiga: &Amiga) -> InterruptDispatchSnapshot {
    let exec_base = read_bus_long(amiga, EXEC_BASE_ADDR);
    let list_ptr = read_bus_long(amiga, exec_base + EXEC_EXTER_DISPATCH_OFFSET);
    let dispatcher = read_bus_long(amiga, exec_base + EXEC_EXTER_DISPATCH_OFFSET + 4);
    let ack_mask = read_bus_word(amiga, list_ptr + 0x0E);

    let mut nodes = Vec::new();
    let mut node_addr = read_bus_long(amiga, list_ptr);
    while node_addr != 0 && nodes.len() < MAX_SERVER_NODES {
        let succ = read_bus_long(amiga, node_addr);
        if succ == 0 {
            break;
        }

        let name_ptr = read_bus_long(amiga, node_addr + 0x0A);
        nodes.push(InterruptServerNode {
            node_addr,
            priority: read_bus_byte(amiga, node_addr + 0x09) as i8,
            name: read_c_string(amiga, name_ptr, 64),
            data: read_bus_long(amiga, node_addr + 0x0E),
            code: read_bus_long(amiga, node_addr + 0x12),
        });
        node_addr = succ;
    }

    InterruptDispatchSnapshot {
        list_ptr,
        dispatcher,
        ack_mask,
        nodes,
    }
}

fn sample_ciab_resource(amiga: &Amiga, base: u32) -> CiabResourceSnapshot {
    let mut slots = Vec::with_capacity(CIAB_RESOURCE_SLOT_COUNT);
    for bit in 0..CIAB_RESOURCE_SLOT_COUNT {
        let slot_addr = base + 0x40 + (bit as u32 * 0x0C);
        let data = read_bus_long(amiga, slot_addr);
        let code = read_bus_long(amiga, slot_addr + 4);
        slots.push(CiabResourceSlot {
            bit: bit as u8,
            slot_addr,
            data,
            code,
            name: read_c_string(amiga, read_bus_long(amiga, slot_addr + 0x0A), 64).or_else(|| {
                if data == 0 {
                    None
                } else {
                    read_c_string(amiga, read_bus_long(amiga, data + 0x0A), 64)
                }
            }),
        });
    }

    CiabResourceSnapshot {
        base,
        field_26: read_bus_word(amiga, base + 0x26),
        field_28: read_bus_byte(amiga, base + 0x28),
        field_29: read_bus_byte(amiga, base + 0x29),
        field_84: read_bus_byte(amiga, base + 0x84),
        slots,
    }
}

fn custom_reg_name(addr: u32) -> Option<&'static str> {
    match addr & 0x01FE {
        0x009A => Some("INTENA"),
        0x009C => Some("INTREQ"),
        _ => None,
    }
}

fn ciab_lvo_name(pc: u32) -> Option<&'static str> {
    match pc {
        0x00FC46F8 => Some("AddICRVector"),
        0x00FC474E => Some("RemICRVector"),
        0x00FC4772 => Some("AbleICR"),
        0x00FC4790 => Some("SetICR"),
        _ => None,
    }
}

fn io_component(addr: u32) -> Option<&'static str> {
    if (0xDD_0000..0xDE_0000).contains(&addr) {
        Some("dmac")
    } else if (0xBFD0_00..0xBFE0_00).contains(&addr) {
        Some("cia_b")
    } else {
        None
    }
}

fn bus_write_value(addr: u32, is_word: bool, data: u16, component: Option<&'static str>) -> u32 {
    if is_word {
        return u32::from(data);
    }

    match component {
        Some("cia_b") => u32::from(data as u8),
        _ if addr & 1 == 0 => u32::from(data >> 8),
        _ => u32::from(data & 0x00FF),
    }
}

fn ciab_reg_name(reg: u8) -> &'static str {
    match reg & 0x0F {
        0x08 => "TODLO",
        0x09 => "TODMID",
        0x0A => "TODHI",
        0x0D => "ICR",
        0x0E => "CRA",
        0x0F => "CRB",
        _ => "OTHER",
    }
}

fn ciab_should_trace_reg(reg: u8) -> bool {
    matches!(reg & 0x0F, 0x08..=0x0A | 0x0D..=0x0F)
}

fn ciab_control_state_changed(old: CiaSnapshot, new: CiaSnapshot) -> bool {
    old.icr_status != new.icr_status
        || old.icr_mask != new.icr_mask
        || old.cra != new.cra
        || old.crb != new.crb
        || old.tod_alarm != new.tod_alarm
        || old.tod_halted != new.tod_halted
}

fn ciab_helper_kind(pc: u32) -> Option<u32> {
    match pc {
        0x00FF_DEF0 | 0x00FF_DF04 | 0x00FF_DF12 | 0x00FF_DF20 | 0x00FF_DFD4 => Some(pc),
        _ => None,
    }
}

fn run_irq6_trace() {
    let Some(mut amiga) = build_amiga() else {
        return;
    };

    let mut pending_tick = None;
    let mut previous_dmac = sample_dmac(&amiga);
    let mut prev_bus_sig: Option<(u32, u8, bool, bool, Option<u16>)> = None;
    let mut prev_pc_sig = None;
    let mut prev_task_state = u32::from(read_bus_byte(&amiga, TASK_ADDR + 0x0F));
    let mut prev_task_sig_recvd = read_bus_long(&amiga, TASK_ADDR + 0x1A);
    let mut prev_cia_b = sample_cia(&amiga.cia_b);
    let mut ciab_lvo_events = Vec::new();
    let mut ciab_control_events = Vec::new();
    let mut ciab_state_changes = Vec::new();
    let mut ciab_helper_events = Vec::new();
    let mut pending_board_io_events = Vec::new();
    let stop_resume_pc = 0x00F81496;

    let mut report = None;

    for tick in 0..MAX_BOOT_TICKS {
        amiga.tick();

        let current_dmac = sample_dmac(&amiga);
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

        if pending_tick.is_none() && current_dmac.istr & 0x10 != 0 {
            let exec_base = read_bus_long(&amiga, EXEC_BASE_ADDR);
            let exter_dispatch = sample_exter_dispatch(&amiga);
            let ciab_resource = exter_dispatch
                .nodes
                .iter()
                .find(|node| node.name.as_deref() == Some("ciab.resource"))
                .map(|node| sample_ciab_resource(&amiga, node.data));
            pending_tick = Some(tick);
            report = Some(TraceReport {
                rom_path: "../../roms/kick31_40_068_a3000.rom",
                pending_tick: tick,
                pending_state: sample_cpu(&amiga, tick),
                pending_dmac: current_dmac,
                exec_base,
                pending_cia_b: sample_cia(&amiga.cia_b),
                exter_dispatch,
                ciab_resource,
                ciab_lvo_events: std::mem::take(&mut ciab_lvo_events),
                ciab_control_events: std::mem::take(&mut ciab_control_events),
                ciab_state_changes: std::mem::take(&mut ciab_state_changes),
                ciab_helper_events: std::mem::take(&mut ciab_helper_events),
                stop_tick: None,
                iack_events: Vec::new(),
                dmac_changes: Vec::new(),
                board_io_events: std::mem::take(&mut pending_board_io_events),
                custom_write_events: Vec::new(),
                server_calls: Vec::new(),
                task_events: Vec::new(),
                pc_samples: vec![sample_cpu(&amiga, tick)],
            });
            prev_pc_sig = Some((amiga.cpu.regs.pc, amiga.cpu.instr_start_pc, amiga.cpu.ir));
        }

        let Some(pending_tick) = pending_tick else {
            if let Some(lvo) = ciab_lvo_name(amiga.cpu.instr_start_pc) {
                if ciab_lvo_events.len() < MAX_CIAB_LVO_EVENTS {
                    let interrupt_name = read_c_string(
                        &amiga,
                        read_bus_long(&amiga, amiga.cpu.regs.a(1) + 0x0A),
                        64,
                    );
                    ciab_lvo_events.push(CiabLvoEvent {
                        tick,
                        lvo,
                        pc: amiga.cpu.regs.pc,
                        instr_start_pc: amiga.cpu.instr_start_pc,
                        d0: amiga.cpu.regs.d[0],
                        a1: amiga.cpu.regs.a(1),
                        a6: amiga.cpu.regs.a(6),
                        interrupt_name,
                    });
                }
            }

            if ciab_helper_kind(amiga.cpu.instr_start_pc).is_some()
                && ciab_helper_events.len() < MAX_CIAB_HELPER_EVENTS
            {
                let stack_return_pc = read_bus_long(&amiga, amiga.cpu.regs.a(7));
                let already_recorded = ciab_helper_events.iter().any(|event| {
                    event.instr_start_pc == amiga.cpu.instr_start_pc
                        && event.stack_return_pc == stack_return_pc
                        && event.d0 == amiga.cpu.regs.d[0]
                });
                if !already_recorded {
                    ciab_helper_events.push(CiabHelperEvent {
                        tick,
                        pc: amiga.cpu.regs.pc,
                        instr_start_pc: amiga.cpu.instr_start_pc,
                        stack_return_pc,
                        d0: amiga.cpu.regs.d[0],
                        d1: amiga.cpu.regs.d[1],
                        a0: amiga.cpu.regs.a(0),
                        a1: amiga.cpu.regs.a(1),
                        a6: amiga.cpu.regs.a(6),
                        sr: amiga.cpu.regs.sr,
                        cia: sample_cia(&amiga.cia_b),
                        dmac: current_dmac,
                    });
                }
            }

            let current_cia_b = sample_cia(&amiga.cia_b);
            if ciab_control_state_changed(prev_cia_b, current_cia_b)
                && ciab_state_changes.len() < MAX_CIAB_STATE_CHANGES
            {
                ciab_state_changes.push(CiabStateChangeEvent {
                    tick,
                    old: prev_cia_b,
                    new: current_cia_b,
                    pc: amiga.cpu.regs.pc,
                    instr_start_pc: amiga.cpu.instr_start_pc,
                    ir: amiga.cpu.ir,
                    sr: amiga.cpu.regs.sr,
                    bus_addr: current_bus_sig.map(|(addr, _, _, _, _)| addr),
                    bus_is_read: current_bus_sig.map(|(_, _, is_read, _, _)| is_read),
                    bus_value: current_bus_sig.and_then(|(addr, _, is_read, is_word, data)| {
                        if is_read {
                            None
                        } else {
                            Some(bus_write_value(
                                addr,
                                is_word,
                                data.unwrap_or(0),
                                io_component(addr),
                            ) as u8)
                        }
                    }),
                    dmac: current_dmac,
                });
            }
            prev_cia_b = current_cia_b;

            if current_bus_sig != prev_bus_sig
                && pending_board_io_events.len() < MAX_BOARD_IO_EVENTS
                && let Some((addr, _, is_read, is_word, data)) = current_bus_sig
            {
                let component = io_component(addr);

                if component == Some("cia_b") {
                    let reg = ((addr >> 8) & 0x0F) as u8;
                    if ciab_should_trace_reg(reg)
                        && ciab_control_events.len() < MAX_CIAB_CONTROL_EVENTS
                    {
                        ciab_control_events.push(CiabControlEvent {
                            tick,
                            reg,
                            reg_name: ciab_reg_name(reg),
                            is_read,
                            value: if is_read {
                                None
                            } else {
                                Some(bus_write_value(addr, is_word, data.unwrap_or(0), component)
                                    as u8)
                            },
                            pc: amiga.cpu.regs.pc,
                            instr_start_pc: amiga.cpu.instr_start_pc,
                            ir: amiga.cpu.ir,
                            sr: amiga.cpu.regs.sr,
                            cia: sample_cia(&amiga.cia_b),
                            dmac: current_dmac,
                        });
                    }
                }

                if let Some(component) = component {
                    let effective_data = if is_read {
                        None
                    } else {
                        Some(bus_write_value(
                            addr,
                            is_word,
                            data.unwrap_or(0),
                            Some(component),
                        ))
                    };

                    pending_board_io_events.push(BoardIoEvent {
                        tick,
                        component,
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
            prev_bus_sig = current_bus_sig;
            continue;
        };
        let report = report.as_mut().expect("report starts with pending tick");

        if ciab_helper_kind(amiga.cpu.instr_start_pc).is_some()
            && report.ciab_helper_events.len() < MAX_CIAB_HELPER_EVENTS
        {
            let stack_return_pc = read_bus_long(&amiga, amiga.cpu.regs.a(7));
            let already_recorded = report.ciab_helper_events.iter().any(|event| {
                event.instr_start_pc == amiga.cpu.instr_start_pc
                    && event.stack_return_pc == stack_return_pc
                    && event.d0 == amiga.cpu.regs.d[0]
            });
            if !already_recorded {
                report.ciab_helper_events.push(CiabHelperEvent {
                    tick,
                    pc: amiga.cpu.regs.pc,
                    instr_start_pc: amiga.cpu.instr_start_pc,
                    stack_return_pc,
                    d0: amiga.cpu.regs.d[0],
                    d1: amiga.cpu.regs.d[1],
                    a0: amiga.cpu.regs.a(0),
                    a1: amiga.cpu.regs.a(1),
                    a6: amiga.cpu.regs.a(6),
                    sr: amiga.cpu.regs.sr,
                    cia: sample_cia(&amiga.cia_b),
                    dmac: current_dmac,
                });
            }
        }

        let current_cia_b = sample_cia(&amiga.cia_b);
        if ciab_control_state_changed(prev_cia_b, current_cia_b)
            && report.ciab_state_changes.len() < MAX_CIAB_STATE_CHANGES
        {
            report.ciab_state_changes.push(CiabStateChangeEvent {
                tick,
                old: prev_cia_b,
                new: current_cia_b,
                pc: amiga.cpu.regs.pc,
                instr_start_pc: amiga.cpu.instr_start_pc,
                ir: amiga.cpu.ir,
                sr: amiga.cpu.regs.sr,
                bus_addr: current_bus_sig.map(|(addr, _, _, _, _)| addr),
                bus_is_read: current_bus_sig.map(|(_, _, is_read, _, _)| is_read),
                bus_value: current_bus_sig.and_then(|(addr, _, is_read, is_word, data)| {
                    if is_read {
                        None
                    } else {
                        Some(
                            bus_write_value(addr, is_word, data.unwrap_or(0), io_component(addr))
                                as u8,
                        )
                    }
                }),
                dmac: current_dmac,
            });
        }
        prev_cia_b = current_cia_b;

        if report.stop_tick.is_none()
            && amiga.cpu.regs.pc == stop_resume_pc
            && amiga.cpu.ir == 0x4E72
        {
            report.stop_tick = Some(tick);
        }

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

        let current_task_state = u32::from(read_bus_byte(&amiga, TASK_ADDR + 0x0F));
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

        let current_task_sig_recvd = read_bus_long(&amiga, TASK_ADDR + 0x1A);
        if current_task_sig_recvd != prev_task_sig_recvd
            && report.task_events.len() < MAX_TASK_EVENTS
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

        for node in &report.exter_dispatch.nodes {
            if report.server_calls.len() >= MAX_SERVER_CALLS {
                break;
            }
            let already_recorded = report
                .server_calls
                .iter()
                .any(|call| call.node_addr == node.node_addr);
            if !already_recorded
                && (amiga.cpu.regs.pc == node.code || amiga.cpu.instr_start_pc == node.code)
            {
                report.server_calls.push(ServerCallEvent {
                    tick,
                    node_addr: node.node_addr,
                    name: node.name.clone(),
                    data: node.data,
                    code: node.code,
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

        if let Some(ciab_resource) = report.ciab_resource.as_ref() {
            for slot in &ciab_resource.slots {
                if report.server_calls.len() >= MAX_SERVER_CALLS {
                    break;
                }
                let already_recorded = report
                    .server_calls
                    .iter()
                    .any(|call| call.code == slot.code);
                if slot.code != 0
                    && !already_recorded
                    && (amiga.cpu.regs.pc == slot.code || amiga.cpu.instr_start_pc == slot.code)
                {
                    report.server_calls.push(ServerCallEvent {
                        tick,
                        node_addr: slot.slot_addr,
                        name: Some(format!("ciab.slot{}", slot.bit)),
                        data: slot.data,
                        code: slot.code,
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
                    if report.iack_events.len() == 1 {
                        let call_already_recorded = report
                            .server_calls
                            .iter()
                            .any(|call| call.code == report.exter_dispatch.dispatcher);
                        if !call_already_recorded && report.server_calls.len() < MAX_SERVER_CALLS {
                            report.server_calls.push(ServerCallEvent {
                                tick,
                                node_addr: report.exter_dispatch.list_ptr,
                                name: Some(String::from("Exec interrupt dispatcher")),
                                data: report.exter_dispatch.list_ptr,
                                code: report.exter_dispatch.dispatcher,
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

                let effective_data = if is_read {
                    None
                } else {
                    Some(bus_write_value(
                        addr,
                        is_word,
                        data.unwrap_or(0),
                        io_component(addr),
                    ))
                };

                let component = io_component(addr);

                if component == Some("cia_b") {
                    let reg = ((addr >> 8) & 0x0F) as u8;
                    if ciab_should_trace_reg(reg)
                        && report.ciab_control_events.len() < MAX_CIAB_CONTROL_EVENTS
                    {
                        report.ciab_control_events.push(CiabControlEvent {
                            tick,
                            reg,
                            reg_name: ciab_reg_name(reg),
                            is_read,
                            value: if is_read {
                                None
                            } else {
                                Some(bus_write_value(addr, is_word, data.unwrap_or(0), component)
                                    as u8)
                            },
                            pc: amiga.cpu.regs.pc,
                            instr_start_pc: amiga.cpu.instr_start_pc,
                            ir: amiga.cpu.ir,
                            sr: amiga.cpu.regs.sr,
                            cia: sample_cia(&amiga.cia_b),
                            dmac: current_dmac,
                        });
                    }
                }

                if let Some(component) = component
                    && report.board_io_events.len() < MAX_BOARD_IO_EVENTS
                {
                    report.board_io_events.push(BoardIoEvent {
                        tick,
                        component,
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
                } else if !is_read
                    && report.custom_write_events.len() < MAX_CUSTOM_WRITE_EVENTS
                    && (0xDFF000..0xDFF200).contains(&addr)
                    && let (Some(reg), Some(effective_data)) =
                        (custom_reg_name(addr), effective_data)
                {
                    report.custom_write_events.push(CustomWriteEvent {
                        tick,
                        reg,
                        addr,
                        size: if is_word { "word" } else { "byte" },
                        raw_data: data.unwrap_or(0),
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
