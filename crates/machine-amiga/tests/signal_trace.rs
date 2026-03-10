//! Trace writes to `exec.library.tc_sig_recvd` for the A3000 boot wait loop.
//!
//! This test is ignored by default because it requires a local Kickstart ROM
//! and writes JSON under `test_output/amiga/traces/`.

mod common;

use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;

use machine_amiga::{Amiga, AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};
use serde::Serialize;

use common::load_rom;

const MAX_BOOT_TICKS: u64 = 130_000_000;
const STOP_RESUME_PC: u32 = 0x00F8_1496;
const MAX_SIGNAL_EVENTS: usize = 32;

#[derive(Serialize)]
struct TraceReport {
    rom_path: &'static str,
    target_task: u32,
    pending_tick: Option<u64>,
    stop_tick: u64,
    signal_events: Vec<SignalEvent>,
}

#[derive(Clone, Copy, Serialize)]
struct SignalEvent {
    tick: u64,
    phase: &'static str,
    pc: u32,
    instr_start_pc: u32,
    current_entry: u32,
    d0: u32,
    a1: u32,
    a6: u32,
    stack_return_pc: u32,
    intena: u16,
    intreq: u16,
    old_task_sig_recvd: u32,
    task_sig_wait: u32,
    new_task_sig_recvd: u32,
    dmac_istr: u8,
    dmac_wd_status: u8,
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
    let data = serde_json::to_vec_pretty(report).expect("serialize signal trace report");
    fs::write(&path, data).expect("write signal trace report");
    println!("signal trace saved to {}", path.display());
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

fn discover_target_task() -> Option<(u32, u64)> {
    let mut amiga = build_amiga()?;
    for tick in 0..MAX_BOOT_TICKS {
        amiga.tick();
        if amiga.cpu.regs.pc == STOP_RESUME_PC && amiga.cpu.ir == 0x4E72 {
            return Some((amiga.cpu.regs.a(1), tick));
        }
    }
    None
}

fn run_signal_trace() {
    let Some((target_task, stop_tick)) = discover_target_task() else {
        return;
    };
    let Some(mut amiga) = build_amiga() else {
        return;
    };

    let mut pending_tick = None;
    let mut signal_events = VecDeque::with_capacity(MAX_SIGNAL_EVENTS);
    let mut prev_task_sig_recvd = 0u32;

    for tick in 0..MAX_BOOT_TICKS {
        amiga.tick();

        let dmac = amiga.dmac.as_ref().expect("A3000 should expose SDMAC");
        if pending_tick.is_none() && (dmac.current_istr() & 0x10) != 0 {
            pending_tick = Some(tick);
        }

        let current_task_sig_recvd = read_bus_long(&amiga, target_task + 0x1A);
        if current_task_sig_recvd != prev_task_sig_recvd {
            if signal_events.len() == MAX_SIGNAL_EVENTS {
                signal_events.pop_front();
            }
            signal_events.push_back(SignalEvent {
                tick,
                phase: if pending_tick.is_some_and(|pending| tick >= pending) {
                    "after_pending"
                } else {
                    "before_pending"
                },
                pc: amiga.cpu.regs.pc,
                instr_start_pc: amiga.cpu.instr_start_pc,
                current_entry: read_bus_long(&amiga, amiga.cpu.regs.a(6) + 0x114),
                d0: amiga.cpu.regs.d[0],
                a1: amiga.cpu.regs.a(1),
                a6: amiga.cpu.regs.a(6),
                stack_return_pc: read_bus_long(&amiga, amiga.cpu.regs.a(7)),
                intena: amiga.paula.intena,
                intreq: amiga.paula.intreq,
                old_task_sig_recvd: prev_task_sig_recvd,
                task_sig_wait: read_bus_long(&amiga, target_task + 0x16),
                new_task_sig_recvd: current_task_sig_recvd,
                dmac_istr: dmac.current_istr(),
                dmac_wd_status: dmac.wd_scsi_status(),
            });
            prev_task_sig_recvd = current_task_sig_recvd;
        }

        if tick >= stop_tick {
            break;
        }
    }

    let report = TraceReport {
        rom_path: "../../roms/kick31_40_068_a3000.rom",
        target_task,
        pending_tick,
        stop_tick,
        signal_events: signal_events.into_iter().collect(),
    };
    write_report("signal_trace_kick31_a3000", &report);
}

#[test]
#[ignore]
fn trace_exec_wait_signal_a3000() {
    run_signal_trace();
}
