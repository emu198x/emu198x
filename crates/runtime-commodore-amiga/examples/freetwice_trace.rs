//! Find the cause of AN_FreeTwice ($81000009) during boot.
//!
//! `bootblock_writers` traced the wrong-value writes back to the Exec
//! Alert path at $FC1788 (from the Wandel V33 Exec disassembly). The
//! alert code $81000009 = AN_FreeTwice means Exec's Deallocate detected
//! that a memory block was freed twice.
//!
//! This trace instruments the Exec LVOs involved (AllocMem, FreeMem,
//! Deallocate, Disable, Enable, Forbid, Permit, Alert) plus a few
//! related ones (Cause, Signal). For each LVO entry it captures:
//!   - tick, calling PC (return address from stack), registers, args
//!   - IDNestCnt, TDNestCnt (Exec-level interrupt/task-switch nesting)
//!   - 68000 SR (interrupt mask + supervisor flag)
//!   - Paula's current IPL
//!
//! Capture stops when PC reaches the Alert call at $FC1788 (the V33
//! AN_FreeTwice site). The last ~80 events before that are the
//! suspect chain. Two patterns to look for:
//!
//!   1. A FreeMem call with IDNestCnt >= 0 (interrupts disabled) that
//!      is then interrupted by an IRQ delivering another FreeMem call.
//!      → indicates we deliver IRQs through Disable() → 68000 SR
//!      interrupt-mask gate is broken somewhere.
//!
//!   2. Two FreeMem calls with the same A1 (block address) at distinct
//!      callers. → indicates a logic bug producing a duplicate free,
//!      which itself usually has timing roots (timer.device firing
//!      twice, ISR re-entering).

use emu198x_shell::{MediaKind, read_media_asset};
use machine_commodore_amiga::Amiga;
use std::collections::VecDeque;
use std::fs;
use std::path::Path;

const EXEC_BASE: u32 = 0x00C00276;
const ALERT_CALL_SITE: u32 = 0x00FC1788;

#[derive(Clone)]
struct Event {
    tick: u64,
    lvo: &'static str,
    caller_pc: u32,
    a0: u32,
    a1: u32,
    d0: u32,
    d1: u32,
    d7: u32,
    sp: u32,
    sr: u16,
    paula_ipl: u8,
    id_nest_cnt: i8,
    td_nest_cnt: i8,
    supervisor: bool,
}

fn main() {
    let kickstart = fs::read("/Users/stevehill/.emu198x/roms/commodore-amiga/kick13.rom").unwrap();
    let mut amiga = Amiga::new_with_slow_ram(kickstart, 512 * 1024);

    let disk_path = "/Users/stevehill/Projects/Emu198x-Unclean/Reference/amiga/Operating Systems/Workbench/Workbench v1.3.3 rev 34.34 (1990)(Commodore)(Disk 1 of 2)(Workbench)[Cloanto Amiga Forever Edition].zip";
    let loaded = read_media_asset(Path::new(disk_path), MediaKind::Disk).unwrap();
    let adf = format_commodore_amiga_adf::Adf::from_bytes(loaded.bytes).unwrap();
    amiga.insert_disk(adf);
    amiga.floppy.acknowledge_disk_change();

    // V33/V34 Exec LVO offsets (from ExecBase, negative).
    // Verified against the Wandel exec_disassembly.txt.
    let lvos: &[(&str, u32)] = &[
        ("Disable",    EXEC_BASE.wrapping_sub(0x078)),
        ("Enable",     EXEC_BASE.wrapping_sub(0x07E)),
        ("Forbid",     EXEC_BASE.wrapping_sub(0x084)),
        ("Permit",     EXEC_BASE.wrapping_sub(0x08A)),
        ("Allocate",   EXEC_BASE.wrapping_sub(0x0BA)),
        ("Deallocate", EXEC_BASE.wrapping_sub(0x0C0)),
        ("AllocMem",   EXEC_BASE.wrapping_sub(0x0C6)),
        ("FreeMem",    EXEC_BASE.wrapping_sub(0x0D2)),
        ("Alert",      EXEC_BASE.wrapping_sub(0x06C)),
        ("Cause",      EXEC_BASE.wrapping_sub(0x0B4)),
        ("Signal",     EXEC_BASE.wrapping_sub(0x144)),
    ];

    println!("Watching LVOs:");
    for (name, addr) in lvos {
        println!("  {name:<10} @ ${addr:08X}");
    }
    println!("Stopping at Alert call site $${ALERT_CALL_SITE:08X}");

    let ccks_per_frame = u64::from(amiga.agnus.lines_per_frame)
        * u64::from(commodore_agnus_ocs::PAL_CCKS_PER_LINE);

    let mut events: VecDeque<Event> = VecDeque::with_capacity(800);
    let mut alert_seen_at: Option<u64> = None;
    let mut last_event_pc: u32 = 0;
    let mut alert_freelist_snapshot: Vec<(u32, u32)> = Vec::new();
    let mut alert_block_addr: u32 = 0;
    let mut alert_block_size: u32 = 0;
    let mut alert_memheader: u32 = 0;
    // Capture the conflicting MemChunk address (D3) at one of the three
    // FreeTwice branches inside Deallocate.
    let mut alert_branch_pc: u32 = 0;
    let mut alert_d3_value: u32 = 0;
    let mut alert_a2_value: u32 = 0;

    for tick in 0..(600 * ccks_per_frame) {
        amiga.tick_cck();
        if alert_seen_at.is_some() {
            // Capture a few more events past the alert site for context,
            // then break.
            if tick > alert_seen_at.unwrap() + 200 {
                break;
            }
        }

        let pc = amiga.cpu.instr_start_pc;
        let lvo = lvos.iter().find(|(_, addr)| *addr == pc);
        let at_alert_site = pc == ALERT_CALL_SITE;

        // Catch the three Deallocate FreeTwice branches: snapshot D3/A2/A1
        // at the BEQ/BHI just before the branch is taken to FC177E.
        if alert_branch_pc == 0
            && (pc == 0x00FC172C || pc == 0x00FC1746 || pc == 0x00FC1764)
        {
            // Note: at the branch point, D3 holds the conflicting MemChunk
            // (172C) or the chunk-end address (1746, 1764).
            alert_branch_pc = pc;
            alert_d3_value = amiga.cpu.regs.d[3];
            alert_a2_value = amiga.cpu.regs.a[2];
        }

        if lvo.is_none() && !at_alert_site {
            last_event_pc = pc;
            continue;
        }
        // Dedupe: only fire on PC transition, not on every CCK while the
        // multi-cycle JMP at the LVO trampoline is in progress.
        if pc == last_event_pc {
            continue;
        }
        last_event_pc = pc;

        let sp = amiga.cpu.regs.active_sp();
        let caller_pc = read_long(&amiga, sp);
        let id_nest = amiga.memory.read_byte(EXEC_BASE.wrapping_add(0x126)) as i8;
        let td_nest = amiga.memory.read_byte(EXEC_BASE.wrapping_add(0x127)) as i8;

        let event = Event {
            tick,
            lvo: lvo.map(|(n, _)| *n).unwrap_or("ALERT_SITE"),
            caller_pc,
            a0: amiga.cpu.regs.a[0],
            a1: amiga.cpu.regs.a[1],
            d0: amiga.cpu.regs.d[0],
            d1: amiga.cpu.regs.d[1],
            d7: amiga.cpu.regs.d[7],
            sp,
            sr: amiga.cpu.regs.sr,
            paula_ipl: amiga.cpu.ipl,
            id_nest_cnt: id_nest,
            td_nest_cnt: td_nest,
            supervisor: amiga.cpu.regs.is_supervisor(),
        };

        if events.len() == events.capacity() {
            events.pop_front();
        }
        events.push_back(event);

        if at_alert_site && alert_seen_at.is_none() {
            alert_seen_at = Some(tick);
            // The alert is reached via FC177E (movem.l D7/A5/A6,-(SP);
            // move.l #$81000009,D7). Just before that, A0/A1/D0 still
            // hold Deallocate's working state. Capture the free list.
            alert_block_addr = amiga.cpu.regs.a[1];
            alert_block_size = amiga.cpu.regs.d[0];
            alert_memheader = amiga.cpu.regs.a[0];
            // Walk the free list (mh_First at MemHeader+$10).
            let mut chunk = read_long(&amiga, alert_memheader.wrapping_add(0x10));
            for _ in 0..40 {
                if chunk == 0 || chunk < 0x400 || chunk >= 0xFFFF_FF {
                    break;
                }
                let next = read_long(&amiga, chunk);
                let bytes = read_long(&amiga, chunk.wrapping_add(4));
                alert_freelist_snapshot.push((chunk, bytes));
                if next == 0 {
                    break;
                }
                chunk = next;
            }
            println!("\n!!! Alert call site hit at tick {tick} (D7 = ${:08X}) !!!", amiga.cpu.regs.d[7]);
            println!(
                "  Block being freed: A1=${alert_block_addr:08X}  size=${alert_block_size:08X}  MemHeader A0=${alert_memheader:08X}",
            );
        }
    }

    println!("\nLast {} LVO events (tick, lvo, callerPC, args, sr, ipl, IDNestCnt, TDNestCnt, sup):", events.len());
    println!("           tick    LVO         caller   A0       A1       D0       D1       D7       SP       SR    pIPL ID  TD  S");
    for e in &events {
        println!(
            "  {:>10} {:<11} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:08X} {:04X}   {}  {:>3}  {:>3} {}",
            e.tick,
            e.lvo,
            e.caller_pc,
            e.a0,
            e.a1,
            e.d0,
            e.d1,
            e.d7,
            e.sp,
            e.sr,
            e.paula_ipl,
            e.id_nest_cnt,
            e.td_nest_cnt,
            if e.supervisor { 'S' } else { 'U' },
        );
    }

    // Pattern detection: look for FreeMem/Deallocate calls with the
    // same block address before the Alert site.
    let frees: Vec<(u64, u32, u32, &'static str)> = events
        .iter()
        .filter(|e| matches!(e.lvo, "FreeMem" | "Deallocate"))
        .map(|e| (e.tick, e.a1, e.d0, e.lvo))
        .collect();
    println!("\nFreeMem/Deallocate calls captured: {}", frees.len());
    use std::collections::HashMap;
    let mut by_block: HashMap<u32, Vec<(u64, u32, &'static str)>> = HashMap::new();
    for (tick, addr, size, lvo) in &frees {
        by_block.entry(*addr).or_default().push((*tick, *size, *lvo));
    }
    let mut duplicates: Vec<(u32, Vec<(u64, u32, &'static str)>)> = by_block
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .collect();
    duplicates.sort_by_key(|(addr, _)| *addr);
    println!("Duplicate-block frees ({} unique addresses):", duplicates.len());
    for (addr, calls) in &duplicates {
        println!("  ${addr:08X} freed {} times:", calls.len());
        for (tick, size, lvo) in calls {
            println!("    tick={tick}  size=${size:08X}  via {lvo}");
        }
    }

    // Pattern detection: did any LVO fire with IDNestCnt >= 0 AND
    // came from an interrupt context (caller_pc in IRQ vector range
    // or supervisor mode entered unexpectedly)?
    let interrupted_calls: Vec<&Event> = events
        .iter()
        .filter(|e| {
            e.id_nest_cnt >= 0 // disabled
                && matches!(e.lvo, "FreeMem" | "Deallocate" | "AllocMem" | "Allocate")
        })
        .collect();
    println!("\nMemory-list ops with IDNestCnt >= 0 (interrupts should be disabled): {}", interrupted_calls.len());
    for e in &interrupted_calls {
        println!(
            "  tick={} {} caller=${:08X} A1=${:08X} D0=${:08X} ID={} pIPL={}",
            e.tick, e.lvo, e.caller_pc, e.a1, e.d0, e.id_nest_cnt, e.paula_ipl,
        );
    }

    if alert_seen_at.is_none() {
        println!("\n[ALERT site $FC1788 was NOT reached in 600 frames — extend the run if needed]");
    }

    if alert_branch_pc != 0 {
        let case = match alert_branch_pc {
            0x00FC172C => "EXACT DUPLICATE (cmp D3,A1; beq) — true double-free",
            0x00FC1746 => "PREV-CHUNK OVERLAP (D3 = prev-end; bhi) — list corruption",
            0x00FC1764 => "NEXT-CHUNK OVERLAP (D3 = our-end; bhi) — list corruption",
            _ => "unknown",
        };
        println!("\nFreeTwice branch fired at PC ${alert_branch_pc:08X}: {case}");
        println!("  D3 (conflicting/computed addr): ${alert_d3_value:08X}");
        println!("  A2 (prev MemChunk pointer):     ${alert_a2_value:08X}");
        println!("  A1 (block being freed):         ${alert_block_addr:08X}");
        println!("  D0 (size, rounded to mult of 8):${alert_block_size:08X}");
    } else {
        println!("\n[FreeTwice branch (FC172C/FC1746/FC1764) was NOT detected — Deallocate path may differ in K1.3]");
    }

    if !alert_freelist_snapshot.is_empty() {
        println!(
            "\nFree list at alert (MemHeader ${alert_memheader:08X}, {} chunks):",
            alert_freelist_snapshot.len(),
        );
        for (i, (addr, size)) in alert_freelist_snapshot.iter().enumerate() {
            let end = addr.wrapping_add(*size);
            let marker = if *addr <= alert_block_addr && alert_block_addr < end {
                " <-- contains block being freed!"
            } else if alert_block_addr <= *addr && *addr < alert_block_addr.wrapping_add(alert_block_size) {
                " <-- chunk overlaps end of block being freed!"
            } else {
                ""
            };
            println!(
                "  [{i:2}] ${addr:08X}..${end:08X}  size=${size:08X}{marker}",
            );
        }
    }
}

fn read_long(amiga: &Amiga, addr: u32) -> u32 {
    (u32::from(amiga.memory.read_word(addr)) << 16)
        | u32::from(amiga.memory.read_word(addr.wrapping_add(2)))
}
