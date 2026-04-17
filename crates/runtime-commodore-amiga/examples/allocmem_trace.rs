//! Log every Exec AllocMem call during Kickstart boot.
//!
//! Resolves the real AllocMem entry point from the LVO jump table at
//! ExecBase-$C6, then hooks CPU execution: when PC reaches that address,
//! capture D0 (size), D1 (flags), and the caller's return address off
//! the stack. Track the (return-address, size, flags) triple until the
//! CPU returns to the saved return address, then capture D0 as the
//! returned pointer.
//!
//! Output goes to /tmp/amiga_allocmem_trace.txt — too long to dump inline.

use machine_commodore_amiga::Amiga;
use std::collections::VecDeque;
use std::fs;
use std::io::Write;

fn main() {
    let kickstart = fs::read("/Users/stevehill/.emu198x/roms/commodore-amiga/kick13.rom")
        .expect("read kickstart");
    let mut amiga = Amiga::new_with_slow_ram(kickstart, 512 * 1024);

    // We need Exec to be initialised enough that the LVO table is populated.
    // Run ~200 frames first; ExecBase and its LVOs are set up very early
    // but we want to be past the first block of AllocMem calls.
    //
    // Problem: we need to hook from the VERY FIRST AllocMem call to get a
    // complete trace. The LVO pointers get set up before any AllocMem runs,
    // so we can resolve AllocMem's address by polling every tick until the
    // exec-base pointer at $4 is non-zero AND the LVO at ExecBase-198 has a
    // JMP $4EF9 opcode.

    let read_long = |amiga: &Amiga, a: u32| -> u32 {
        (u32::from(amiga.memory.read_word(a)) << 16)
            | u32::from(amiga.memory.read_word(a.wrapping_add(2)))
    };

    // Tick until we can resolve AllocMem's real entry address.
    let mut allocmem_entry: Option<u32> = None;
    let ccks_per_frame = u64::from(amiga.agnus.lines_per_frame)
        * u64::from(commodore_agnus_ocs::PAL_CCKS_PER_LINE);

    // Resolve-only dry run, up to 300 frames.
    let mut resolved_tick: u64 = 0;
    let mut last_exec_base: u32 = 0;
    for tick in 0..(300 * ccks_per_frame) {
        amiga.tick_cck();
        let exec_base = read_long(&amiga, 0x4);
        if exec_base != last_exec_base {
            eprintln!("[dry] tick={tick:>10} ExecBase=${exec_base:08X}");
            last_exec_base = exec_base;
        }
        if (0x400..0x7_FFE0).contains(&exec_base)
            || (0xC0_0000..0xC8_0000).contains(&exec_base)
        {
            let lvo_addr = exec_base.wrapping_sub(198);
            let jmp_op = amiga.memory.read_word(lvo_addr);
            if jmp_op == 0x4EF9 {
                let target = read_long(&amiga, lvo_addr.wrapping_add(2));
                if (0xF80000..=0xFFFFFF).contains(&target) {
                    allocmem_entry = Some(target);
                    resolved_tick = tick;
                    break;
                }
            }
        }
    }

    let Some(entry) = allocmem_entry else {
        eprintln!("Could not resolve AllocMem entry in 300 frames. Final ExecBase=${:08X}", read_long(&amiga, 0x4));
        return;
    };

    eprintln!(
        "AllocMem entry resolved at ${entry:08X} (tick {resolved_tick}, ExecBase=${:08X})",
        read_long(&amiga, 0x4)
    );

    // Note: the CPU may have already made AllocMem calls before we resolved
    // the entry. So we can't claim a "complete" trace. For a truly complete
    // trace, hook using `exec_base` content at $4 every tick and fire when
    // CPU.instr_start_pc == LVO AllocMem JMP addr (i.e., trap at the LVO,
    // before it JMPs to ROM). We'll do that instead.

    // Reset and re-run, hooking at the LVO address instead of the ROM entry.
    let kickstart = fs::read("/Users/stevehill/.emu198x/roms/commodore-amiga/kick13.rom")
        .expect("read kickstart again");
    let mut amiga = Amiga::new_with_slow_ram(kickstart, 512 * 1024);

    let mut out = std::io::BufWriter::new(
        fs::File::create("/tmp/amiga_allocmem_trace.txt").expect("create trace"),
    );

    #[derive(Debug, Clone)]
    struct Pending {
        seq: u64,
        size: u32,
        flags: u32,
        caller: u32,
    }

    let mut pending: VecDeque<Pending> = VecDeque::new();
    let mut seq: u64 = 0;
    let mut prev_pc: u32 = u32::MAX;
    let mut exec_base_resolved = false;
    let mut allocmem_lvo: Option<u32> = None;
    let mut returns_logged: u64 = 0;

    writeln!(
        out,
        "# AllocMem trace for Kickstart 1.3, A500 OCS PAL, 512 KB chip + 512 KB slow"
    )
    .unwrap();
    writeln!(
        out,
        "# Columns: seq call/ret tick pc size flags caller returned"
    )
    .unwrap();

    let target_frames: u64 = 500;
    for tick in 0..(target_frames * ccks_per_frame) {
        amiga.tick_cck();

        // Resolve AllocMem LVO once ExecBase is up.
        if !exec_base_resolved {
            let exec_base = read_long(&amiga, 0x4);
            if (0x400..0x7_FFE0).contains(&exec_base)
            || (0xC0_0000..0xC8_0000).contains(&exec_base)
        {
                let lvo = exec_base.wrapping_sub(198);
                let jmp_op = amiga.memory.read_word(lvo);
                if jmp_op == 0x4EF9 {
                    allocmem_lvo = Some(lvo);
                    exec_base_resolved = true;
                    writeln!(
                        out,
                        "# ExecBase=${:08X}  AllocMem LVO=${lvo:08X}  ROM entry=${:08X}  tick={tick}",
                        exec_base,
                        read_long(&amiga, lvo.wrapping_add(2))
                    )
                    .unwrap();
                }
            }
        }

        let pc = amiga.cpu.instr_start_pc;
        if pc == prev_pc {
            continue;
        }
        prev_pc = pc;

        // Hook entry: LVO address OR the ROM entry. Trap at LVO so we catch
        // calls from anywhere in ROM.
        if Some(pc) == allocmem_lvo {
            let size = amiga.cpu.regs.d[0];
            let flags = amiga.cpu.regs.d[1];
            // Caller's return address is at (A7): top of stack.
            // For trampoline callers ($FD3C34), the real user is one frame up,
            // accessible via the saved A6 on stack then the return address past it.
            let sp = amiga.cpu.regs.active_sp();
            let caller = read_long(&amiga, sp);
            // Dump top 5 longwords of stack for context on indirect callers.
            let stack_top: [u32; 5] = [
                read_long(&amiga, sp),
                read_long(&amiga, sp.wrapping_add(4)),
                read_long(&amiga, sp.wrapping_add(8)),
                read_long(&amiga, sp.wrapping_add(12)),
                read_long(&amiga, sp.wrapping_add(16)),
            ];
            seq += 1;
            writeln!(
                out,
                "{seq:>4} CALL tick={tick:>10} pc=${pc:08X} size=${size:08X} flags=${flags:08X} caller=${caller:08X} sp=${sp:08X} stack=[${:08X} ${:08X} ${:08X} ${:08X} ${:08X}]",
                stack_top[0], stack_top[1], stack_top[2], stack_top[3], stack_top[4]
            )
            .unwrap();
            pending.push_back(Pending {
                seq,
                size,
                flags,
                caller,
            });
            continue;
        }

        // Hook return: if PC matches any pending caller address, capture D0.
        // Note: multiple pending entries could share the same caller if code
        // calls AllocMem twice in a tight loop; we match FIFO.
        if !pending.is_empty() && pc == pending.front().unwrap().caller {
            let p = pending.pop_front().unwrap();
            let ret_d0 = amiga.cpu.regs.d[0];
            writeln!(
                out,
                "{:>4} RET  tick={tick:>10} pc=${pc:08X} size=${:08X} flags=${:08X} caller=${:08X} returned=${ret_d0:08X}",
                p.seq, p.size, p.flags, p.caller,
            )
            .unwrap();
            returns_logged += 1;
        }
    }

    writeln!(out, "").unwrap();
    writeln!(
        out,
        "# Summary: {} calls logged, {} returns captured. ExecBase=${:08X}. Final outstanding={}.",
        seq,
        returns_logged,
        read_long(&amiga, 0x4),
        pending.len(),
    )
    .unwrap();

    out.flush().unwrap();
    eprintln!(
        "wrote /tmp/amiga_allocmem_trace.txt  ({} calls, {} returns)",
        seq, returns_logged
    );
}
