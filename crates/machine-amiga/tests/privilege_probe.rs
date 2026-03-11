//! Diagnostic probes for the first privilege-violation path during Amiga boot.
//!
//! These tests are ignored by default because they require local Kickstart ROMs
//! and write JSON reports under `test_output/amiga/probes/`.

mod common;

use std::collections::VecDeque;
use std::env;
use std::fs;
use std::path::PathBuf;

use machine_amiga::{Amiga, AmigaChipset, AmigaConfig, AmigaModel, AmigaRegion};
use serde::Serialize;

use common::load_rom;

const DEFAULT_MAX_TICKS: u64 = 120_000_000;
const DEFAULT_FOLLOWUP_TICKS: u64 = 200_000;
const FOLLOWUP_INSTRUCTION_LIMIT: usize = 32;

struct ProbeSpec {
    model: AmigaModel,
    rom_path: &'static str,
    report_name: &'static str,
    slow_ram_size: usize,
}

#[derive(Serialize)]
struct ProbeReport {
    model: &'static str,
    rom_path: &'static str,
    max_ticks: u64,
    followup_ticks: u64,
    initial: CpuSnapshot,
    vector8_before_boot: VectorSnapshot,
    first_privilege_fault: Option<PrivilegeFault>,
    final_state: CpuSnapshot,
}

#[derive(Serialize)]
struct CpuSnapshot {
    master_clock: u64,
    overlay: bool,
    pc: u32,
    instr_start_pc: u32,
    sr: u16,
    usp: u32,
    ssp: u32,
    a7: u32,
    ir: u16,
    irc: u16,
    d0: u32,
    d1: u32,
    a5: u32,
    a6: u32,
    exc_vector: Option<u8>,
    followup_tag: u8,
    halted: bool,
    idle: bool,
}

#[derive(Serialize)]
struct VectorSnapshot {
    logical_vector8: u32,
    physical_chip_vector8: u32,
}

#[derive(Serialize)]
struct PrivilegeFault {
    start_tick: u64,
    handler_entry_tick: u64,
    vector8_at_fault: VectorSnapshot,
    leading_instructions: Vec<InstructionSnapshot>,
    followup_instructions: Vec<InstructionSnapshot>,
    start: CpuSnapshot,
    handler: CpuSnapshot,
    frame: StackFrameSnapshot,
    patched_frame: Option<PatchedFrameSnapshot>,
    a6_vector: ExecVectorSnapshot,
    repeat_faults_within_followup: u32,
    first_repeat: Option<RepeatFault>,
}

#[derive(Serialize)]
struct StackFrameSnapshot {
    stack_base: u32,
    storage: &'static str,
    saved_sr: u16,
    saved_pc: u32,
    format_word: Option<u16>,
    vector_offset: Option<u16>,
    saved_pc_words: Vec<u16>,
    handler_words: Vec<u16>,
}

#[derive(Serialize)]
struct PatchedFrameSnapshot {
    tick: u64,
    saved_pc: u32,
    format_word: Option<u16>,
    resume_words: Vec<u16>,
}

#[derive(Clone, Serialize)]
struct InstructionSnapshot {
    master_clock: u64,
    instr_start_pc: u32,
    pc: u32,
    ir: u16,
    sr: u16,
    usp: u32,
    ssp: u32,
    a7: u32,
}

#[derive(Serialize)]
struct RepeatFault {
    start_tick: u64,
    handler_entry_tick: Option<u64>,
    saved_pc: Option<u32>,
    saved_sr: Option<u16>,
}

#[derive(Serialize)]
struct ExecVectorSnapshot {
    vector_addr: u32,
    words: Vec<u16>,
    abs_target: Option<u32>,
}

fn max_ticks() -> u64 {
    let Some(raw) = env::var_os("AMIGA_PRIVILEGE_PROBE_TICKS") else {
        return DEFAULT_MAX_TICKS;
    };

    raw.to_string_lossy()
        .trim()
        .parse()
        .expect("invalid AMIGA_PRIVILEGE_PROBE_TICKS value")
}

fn followup_ticks() -> u64 {
    let Some(raw) = env::var_os("AMIGA_PRIVILEGE_PROBE_FOLLOWUP_TICKS") else {
        return DEFAULT_FOLLOWUP_TICKS;
    };

    raw.to_string_lossy()
        .trim()
        .parse()
        .expect("invalid AMIGA_PRIVILEGE_PROBE_FOLLOWUP_TICKS value")
}

fn chipset_for_model(model: AmigaModel) -> AmigaChipset {
    match model {
        AmigaModel::A1000 | AmigaModel::A500 | AmigaModel::A2000 => AmigaChipset::Ocs,
        AmigaModel::A500Plus | AmigaModel::A600 | AmigaModel::A3000 => AmigaChipset::Ecs,
        AmigaModel::A1200 | AmigaModel::A4000 => AmigaChipset::Aga,
    }
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

fn output_path(report_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test_output/amiga/probes")
        .join(format!("{report_name}.json"))
}

fn write_report(report_name: &str, report: &ProbeReport) {
    let path = output_path(report_name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create probe output directory");
    }
    let data = serde_json::to_vec_pretty(report).expect("serialize probe report");
    fs::write(&path, data).expect("write probe report");
    println!("Privilege probe saved to {}", path.display());
}

fn read_logical_byte(amiga: &Amiga, addr: u32) -> u8 {
    amiga.memory.read_byte(addr)
}

fn read_logical_long(amiga: &Amiga, addr: u32) -> u32 {
    (u32::from(read_logical_byte(amiga, addr)) << 24)
        | (u32::from(read_logical_byte(amiga, addr.wrapping_add(1))) << 16)
        | (u32::from(read_logical_byte(amiga, addr.wrapping_add(2))) << 8)
        | u32::from(read_logical_byte(amiga, addr.wrapping_add(3)))
}

fn physical_storage(amiga: &Amiga, addr: u32) -> &'static str {
    if !amiga.memory.fast_ram.is_empty() {
        let base = amiga.memory.fast_ram_base;
        let end = base.wrapping_add(amiga.memory.fast_ram.len() as u32);
        if addr >= base && addr < end {
            return "fast_ram";
        }
    }

    let addr24 = addr & 0x00FF_FFFF;
    if addr24 < 0x20_0000 {
        "chip_ram"
    } else if (0xC0_0000..0xE0_0000).contains(&addr24) && !amiga.memory.slow_ram.is_empty() {
        "slow_ram"
    } else if addr24 >= 0xF8_0000 {
        "kickstart"
    } else {
        "unmapped"
    }
}

fn read_physical_byte(amiga: &Amiga, addr: u32) -> u8 {
    if !amiga.memory.fast_ram.is_empty() {
        let base = amiga.memory.fast_ram_base;
        let end = base.wrapping_add(amiga.memory.fast_ram.len() as u32);
        if addr >= base && addr < end {
            let offset = (addr - base) & amiga.memory.fast_ram_mask;
            return amiga.memory.fast_ram[offset as usize];
        }
    }

    let addr24 = addr & 0x00FF_FFFF;
    if addr24 < 0x20_0000 {
        return amiga.memory.chip_ram[(addr24 & amiga.memory.chip_ram_mask) as usize];
    }
    if (0xC0_0000..0xE0_0000).contains(&addr24) && !amiga.memory.slow_ram.is_empty() {
        let offset = (addr24 - 0xC0_0000) & amiga.memory.slow_ram_mask;
        return amiga.memory.slow_ram[offset as usize];
    }
    if addr24 >= 0xF8_0000 {
        return amiga.memory.kickstart[(addr24 & amiga.memory.kickstart_mask) as usize];
    }
    0
}

fn read_physical_word(amiga: &Amiga, addr: u32) -> u16 {
    (u16::from(read_physical_byte(amiga, addr)) << 8)
        | u16::from(read_physical_byte(amiga, addr.wrapping_add(1)))
}

fn read_physical_long(amiga: &Amiga, addr: u32) -> u32 {
    (u32::from(read_physical_word(amiga, addr)) << 16)
        | u32::from(read_physical_word(amiga, addr.wrapping_add(2)))
}

fn read_words(amiga: &Amiga, addr: u32, count: usize) -> Vec<u16> {
    (0..count)
        .map(|i| read_physical_word(amiga, addr.wrapping_add((i as u32) * 2)))
        .collect()
}

fn cpu_snapshot(amiga: &Amiga) -> CpuSnapshot {
    CpuSnapshot {
        master_clock: amiga.master_clock,
        overlay: amiga.memory.overlay,
        pc: amiga.cpu.regs.pc,
        instr_start_pc: amiga.cpu.instr_start_pc,
        sr: amiga.cpu.regs.sr,
        usp: amiga.cpu.regs.usp,
        ssp: amiga.cpu.regs.ssp,
        a7: amiga.cpu.regs.a(7),
        ir: amiga.cpu.ir,
        irc: amiga.cpu.irc,
        d0: amiga.cpu.regs.d[0],
        d1: amiga.cpu.regs.d[1],
        a5: amiga.cpu.regs.a(5),
        a6: amiga.cpu.regs.a(6),
        exc_vector: amiga.cpu.exc_vector,
        followup_tag: amiga.cpu.followup_tag,
        halted: amiga.cpu.is_halted(),
        idle: amiga.cpu.is_idle(),
    }
}

fn vector8_snapshot(amiga: &Amiga) -> VectorSnapshot {
    VectorSnapshot {
        logical_vector8: read_logical_long(amiga, 0x20),
        physical_chip_vector8: read_physical_long(amiga, 0x20),
    }
}

fn stack_frame_snapshot(amiga: &Amiga, handler_instr_start_pc: u32) -> StackFrameSnapshot {
    let stack_base = amiga.cpu.regs.ssp;
    let storage = physical_storage(amiga, stack_base);
    let saved_sr = read_physical_word(amiga, stack_base);
    let saved_pc = read_physical_long(amiga, stack_base.wrapping_add(2));
    let has_format_word = amiga.cpu.model.capabilities().movec;
    let format_word =
        has_format_word.then(|| read_physical_word(amiga, stack_base.wrapping_add(6)));
    let vector_offset = format_word.map(|word| word & 0x0FFF);
    let saved_pc_words = read_words(amiga, saved_pc, 4);
    let handler_words = read_words(amiga, handler_instr_start_pc, 8);

    StackFrameSnapshot {
        stack_base,
        storage,
        saved_sr,
        saved_pc,
        format_word,
        vector_offset,
        saved_pc_words,
        handler_words,
    }
}

fn instruction_snapshot(amiga: &Amiga) -> InstructionSnapshot {
    InstructionSnapshot {
        master_clock: amiga.master_clock,
        instr_start_pc: amiga.cpu.instr_start_pc,
        pc: amiga.cpu.regs.pc,
        ir: amiga.cpu.ir,
        sr: amiga.cpu.regs.sr,
        usp: amiga.cpu.regs.usp,
        ssp: amiga.cpu.regs.ssp,
        a7: amiga.cpu.regs.a(7),
    }
}

fn exec_vector_snapshot(amiga: &Amiga) -> ExecVectorSnapshot {
    let vector_addr = amiga.cpu.regs.a(6).wrapping_sub(42);
    let words = read_words(amiga, vector_addr, 4);
    let abs_target = if words.len() >= 3 && words[0] == 0x4EF9 {
        Some((u32::from(words[1]) << 16) | u32::from(words[2]))
    } else {
        None
    };
    ExecVectorSnapshot {
        vector_addr,
        words,
        abs_target,
    }
}

fn run_probe(spec: &ProbeSpec) {
    let Some(kickstart) = load_rom(spec.rom_path) else {
        return;
    };

    let mut amiga = Amiga::new_with_config(AmigaConfig {
        model: spec.model,
        chipset: chipset_for_model(spec.model),
        region: AmigaRegion::Pal,
        kickstart,
        slow_ram_size: spec.slow_ram_size,
        ide_disk: None,
        scsi_disk: None,
            pcmcia_card: None,
    });

    let initial = cpu_snapshot(&amiga);
    let vector8_before_boot = vector8_snapshot(&amiga);
    let max_ticks = max_ticks();
    let followup_ticks = followup_ticks();
    let mut first_fault: Option<PrivilegeFault> = None;
    let mut pending_start: Option<(u64, CpuSnapshot, VectorSnapshot)> = None;
    let mut prev_exc_vector = amiga.cpu.exc_vector;
    let mut recent_instructions = VecDeque::with_capacity(16);
    let mut last_instr_start_pc = u32::MAX;

    for tick in 0..max_ticks {
        amiga.tick();

        if amiga.cpu.is_idle() && amiga.cpu.instr_start_pc != last_instr_start_pc {
            last_instr_start_pc = amiga.cpu.instr_start_pc;
            if recent_instructions.len() == 16 {
                recent_instructions.pop_front();
            }
            recent_instructions.push_back(instruction_snapshot(&amiga));
        }

        if pending_start.is_none() && amiga.cpu.exc_vector == Some(8) {
            let start = cpu_snapshot(&amiga);
            let vectors = vector8_snapshot(&amiga);
            println!(
                "[{}] first privilege-violation sequence started at tick {}: pc=${:08X} ipc=${:08X} sr=${:04X} ssp=${:08X} usp=${:08X}",
                model_name(spec.model),
                amiga.master_clock,
                start.pc,
                start.instr_start_pc,
                start.sr,
                start.ssp,
                start.usp,
            );
            pending_start = Some((tick, start, vectors));
        } else if let Some((start_tick, start, vectors)) = pending_start.take() {
            if prev_exc_vector == Some(8) && amiga.cpu.exc_vector.is_none() {
                let handler = cpu_snapshot(&amiga);
                let frame = stack_frame_snapshot(&amiga, handler.instr_start_pc);
                println!(
                    "[{}] privilege handler entry at tick {}: handler=${:08X} saved_pc=${:08X} saved_sr=${:04X} format={:?}",
                    model_name(spec.model),
                    amiga.master_clock,
                    handler.pc,
                    frame.saved_pc,
                    frame.saved_sr,
                    frame.format_word,
                );
                let mut repeat_faults_within_followup = 0u32;
                let mut first_repeat = None;
                let mut repeat_pending_start: Option<u64> = None;
                let mut repeat_prev_exc_vector = amiga.cpu.exc_vector;
                let mut followup_instructions = Vec::new();
                let mut followup_last_instr_start_pc = u32::MAX;
                let mut patched_frame = None;

                for _ in 0..followup_ticks {
                    amiga.tick();

                    if amiga.cpu.is_idle()
                        && amiga.cpu.instr_start_pc != followup_last_instr_start_pc
                        && followup_instructions.len() < FOLLOWUP_INSTRUCTION_LIMIT
                    {
                        followup_last_instr_start_pc = amiga.cpu.instr_start_pc;
                        followup_instructions.push(instruction_snapshot(&amiga));
                    }

                    if patched_frame.is_none() {
                        let patched_saved_pc =
                            read_physical_long(&amiga, frame.stack_base.wrapping_add(2));
                        if patched_saved_pc != frame.saved_pc {
                            patched_frame = Some(PatchedFrameSnapshot {
                                tick: amiga.master_clock,
                                saved_pc: patched_saved_pc,
                                format_word: frame.format_word.map(|_| {
                                    read_physical_word(&amiga, frame.stack_base.wrapping_add(6))
                                }),
                                resume_words: read_words(&amiga, patched_saved_pc, 4),
                            });
                            println!(
                                "[{}] handler patched stacked PC at tick {}: ${:08X} -> ${:08X}",
                                model_name(spec.model),
                                amiga.master_clock,
                                frame.saved_pc,
                                patched_saved_pc,
                            );
                        }
                    }

                    if repeat_pending_start.is_none() && amiga.cpu.exc_vector == Some(8) {
                        repeat_pending_start = Some(amiga.master_clock);
                    } else if let Some(repeat_start_tick) = repeat_pending_start {
                        if repeat_prev_exc_vector == Some(8) && amiga.cpu.exc_vector.is_none() {
                            repeat_faults_within_followup += 1;
                            let repeat_frame =
                                stack_frame_snapshot(&amiga, amiga.cpu.instr_start_pc);
                            if first_repeat.is_none() {
                                first_repeat = Some(RepeatFault {
                                    start_tick: repeat_start_tick,
                                    handler_entry_tick: Some(amiga.master_clock),
                                    saved_pc: Some(repeat_frame.saved_pc),
                                    saved_sr: Some(repeat_frame.saved_sr),
                                });
                            }
                            repeat_pending_start = None;
                        }
                    }

                    repeat_prev_exc_vector = amiga.cpu.exc_vector;
                }

                first_fault = Some(PrivilegeFault {
                    start_tick: start.master_clock,
                    handler_entry_tick: handler.master_clock,
                    vector8_at_fault: vectors,
                    leading_instructions: recent_instructions.iter().cloned().collect(),
                    followup_instructions,
                    start,
                    handler,
                    frame,
                    patched_frame,
                    a6_vector: exec_vector_snapshot(&amiga),
                    repeat_faults_within_followup,
                    first_repeat,
                });
                break;
            }
            pending_start = Some((start_tick, start, vectors));
        }

        prev_exc_vector = amiga.cpu.exc_vector;
    }

    if first_fault.is_none() {
        println!(
            "[{}] no privilege violation observed within {} ticks",
            model_name(spec.model),
            max_ticks
        );
    }

    let report = ProbeReport {
        model: model_name(spec.model),
        rom_path: spec.rom_path,
        max_ticks,
        followup_ticks,
        initial,
        vector8_before_boot,
        first_privilege_fault: first_fault,
        final_state: cpu_snapshot(&amiga),
    };

    write_report(spec.report_name, &report);
}

#[test]
#[ignore]
fn probe_first_privilege_fault_a500() {
    run_probe(&ProbeSpec {
        model: AmigaModel::A500,
        rom_path: "../../roms/kick13.rom",
        report_name: "privilege_probe_kick13_a500",
        slow_ram_size: 512 * 1024,
    });
}

#[test]
#[ignore]
fn probe_first_privilege_fault_a1200() {
    run_probe(&ProbeSpec {
        model: AmigaModel::A1200,
        rom_path: "../../roms/kick31_40_068_a1200.rom",
        report_name: "privilege_probe_kick31_a1200",
        slow_ram_size: 0,
    });
}

#[test]
#[ignore]
fn probe_first_privilege_fault_a3000() {
    run_probe(&ProbeSpec {
        model: AmigaModel::A3000,
        rom_path: "../../roms/kick31_40_068_a3000.rom",
        report_name: "privilege_probe_kick31_a3000",
        slow_ram_size: 0,
    });
}
