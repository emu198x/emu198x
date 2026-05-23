//! Stage C of the A1200 rollout (see
//! `knowledge/decisions/amiga-machine-rollout-plan.md`).
//!
//! Loads the real Kickstart 3.1 ROM (Cloanto / Hyperion-licensed, user-
//! supplied) into the A1200 machine with `Cpu68020` swapped in, runs N
//! frames, and reports where the boot stops, hangs, or faults. The
//! deliverable is the *first observed failure* — Stage D plans the
//! fix from whatever this test surfaces.
//!
//! ROM lookup order:
//! 1. `$EMU198X_KS31_A1200_ROM` env var (explicit path).
//! 2. `~/.emu198x/roms/commodore-amiga/kick31a1200.rom` (default).
//!
//! If neither resolves the test skips loudly with `eprintln!` rather
//! than failing — KS 3.1 is not redistributable and CI machines
//! without the user's licensed copy should still pass the suite.

use machine_commodore_amiga_a1200::{AmigaA1200, PAL_FRAME_TICKS, RamConfig};
use std::path::PathBuf;

fn load_ks31_rom() -> Option<Vec<u8>> {
    let path = match std::env::var("EMU198X_KS31_A1200_ROM") {
        Ok(p) => PathBuf::from(p),
        Err(_) => {
            let home = std::env::var("HOME").expect("HOME is set");
            PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick31a1200.rom")
        }
    };
    if !path.exists() {
        eprintln!(
            "skipping: KS 3.1 A1200 ROM missing at {} (set $EMU198X_KS31_A1200_ROM to override)",
            path.display()
        );
        return None;
    }
    let bytes = std::fs::read(&path).expect("read KS 3.1 ROM");
    eprintln!(
        "loaded KS 3.1 A1200 ROM: {} bytes from {}",
        bytes.len(),
        path.display()
    );
    Some(bytes)
}

fn a1200_2mb_chip(rom: Vec<u8>) -> AmigaA1200 {
    AmigaA1200::with_ram_config(
        rom,
        RamConfig {
            chip_kb: 2048,
            slow_kb: 0,
            fast_kb: 0,
        },
    )
}

/// Run for `frames` PAL frames and report the CPU state, focusing on
/// what's visible at the failure boundary.
fn report_state(label: &str, m: &AmigaA1200, frames: u64) {
    let cpu = m.cpu();
    eprintln!("--- {label} after {frames} frames ---");
    eprintln!("  PC = ${:08X}", cpu.regs.pc);
    eprintln!(
        "  SR = ${:04X} ({}supervisor, IPL mask {})",
        cpu.regs.sr,
        if cpu.regs.is_supervisor() {
            ""
        } else {
            "user — NOT "
        },
        cpu.regs.interrupt_mask()
    );
    eprintln!("  USP=${:08X} SSP=${:08X}", cpu.regs.usp, cpu.regs.ssp);
    eprintln!(
        "  D0..D7 = {}",
        (0..8)
            .map(|i| format!("${:08X}", cpu.regs.d[i]))
            .collect::<Vec<_>>()
            .join(" ")
    );
    eprintln!(
        "  A0..A6 = {} A7=${:08X} (active SP)",
        (0..7)
            .map(|i| format!("${:08X}", cpu.regs.a[i]))
            .collect::<Vec<_>>()
            .join(" "),
        if cpu.regs.is_supervisor() {
            cpu.regs.ssp
        } else {
            cpu.regs.usp
        }
    );
    eprintln!(
        "  VBR=${:08X} SFC={} DFC={}",
        cpu.regs.vbr, cpu.regs.sfc, cpu.regs.dfc
    );
}

/// Dump the next ~16 bytes of code starting at `pc`, formatted as a
/// run of words for manual disassembly.
fn dump_code_at(m: &AmigaA1200, pc: u32, words: u32) {
    eprintln!("  code @ ${pc:08X}:");
    eprint!("   ");
    for i in 0..words {
        let w = m.read_word(pc.wrapping_add(i * 2));
        eprint!(" {:04X}", w);
    }
    eprintln!();
}

#[test]
fn ks31_boots_far_enough_to_advance_pc_past_reset_vector() {
    let Some(rom) = load_ks31_rom() else { return };

    let mut m = a1200_2mb_chip(rom);

    let initial_pc = m.cpu().regs.pc;
    eprintln!("initial PC after reset_to: ${initial_pc:08X}");
    assert_ne!(initial_pc, 0, "PC should not be zero after reset_to");
    assert!(
        (0x00F8_0000..0x0100_0000).contains(&initial_pc),
        "initial PC ${initial_pc:08X} should sit in the ROM window $F80000-$FFFFFF"
    );

    // Track unique PCs visited over the run — a tight loop will show
    // a small number despite many ticks; healthy boot shows hundreds
    // or thousands.
    let mut unique_pcs = std::collections::BTreeSet::new();
    let mut last_pc_in_rom: u32 = initial_pc;
    let mut excursion_count: u64 = 0;

    // Sample PC + IPL + VBR at every checkpoint. KS 3.x lowers the
    // CPU IPL mask once init reaches the "interrupts on" phase and
    // moves VBR to its chip-RAM exception table. Those transitions
    // are the most informative progress signals.
    // Stage J: 2000 frames is enough — vec 11 fires within the first
    // few hundred frames. We need the chip[$002C] and OVL snapshot at
    // first fire, not a long stress run.
    let frames_to_run: u64 = 2_000;
    let checkpoint_every: u64 = 500;
    // Tracked but not currently read — kept for future "PC moved
    // since last checkpoint?" diagnostic without disturbing the
    // surrounding investigation loop.
    let mut _last_checkpoint_pc = initial_pc;
    let mut min_ipl_seen = 7u8;
    let mut first_vbr_change_frame: Option<u64> = None;
    let mut first_ipl_drop_frame: Option<u64> = None;
    // Exception tracking: count None -> Some(vector) transitions on
    // the cpu.exc_vector field. The field stays Some for the duration
    // of exception processing (multiple ticks), so the edge tells us
    // when a *new* exception was taken. PC-at-edge gives the address
    // that triggered.
    let mut exc_counts: std::collections::HashMap<u8, u64> = std::collections::HashMap::new();
    let mut exc_first_pc: std::collections::HashMap<u8, u32> = std::collections::HashMap::new();
    let mut prev_exc: Option<u8> = m.cpu().exc_vector;
    // Hot PCs: sample PC at every 128th tick (~7M samples over 5000
    // PAL frames). The hottest PCs reveal which loops are eating
    // emulated time.
    let mut hot_pcs: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    let mut tick_counter: u64 = 0;
    // Track entries into $F83182 (the byte-receive routine that leads
    // to the Wack loop). On the rising edge of PC == $F83182, read
    // the return address from the supervisor stack to identify the
    // caller.
    let mut prev_pc = m.cpu().regs.pc;
    let mut byte_receive_entries: std::collections::HashMap<u32, u64> =
        std::collections::HashMap::new();
    // Track each time the PC FIRST crosses into the Wack
    // prologue range ($F8325E-$F832B0). Capture (prev_pc, entry_pc,
    // SSP top 8 bytes) on each transition — the entry path tells us
    // whether it's a RTE-landing, an exception, or a fall-through.
    let mut wack_entries: Vec<(u32, u32, u32, u32)> = Vec::new();
    let wack_entry_lo = 0x00F8_325Eu32;
    let wack_entry_hi = 0x00F8_32B0u32;
    let mut diagalive_entries: Vec<(u32, u32, u32, u32)> = Vec::new();
    // Stage H: track entries into the $F835F0 reboot-loop init function.
    // $F835F0 is where KS sets LEA $0400, A7 — a fresh supervisor stack
    // setup that suggests this is a "stage-1" boot entry. If we can
    // identify what JUMPS / BRANCHES to it, we know which earlier
    // boot decision routes us into the perpetual-reboot path.
    let mut reboot_init_entries: Vec<(u32, u32)> = Vec::new();
    // Track the $F800D0 reset re-entry too (the $F80DB8 routine's JMP).
    let mut reset_reentries: Vec<u32> = Vec::new();
    // Stage I: capture the FIRST time D7's bit 31 gets set during a
    // boot cycle. The PC at that moment is the instruction that
    // caused it. Re-arm on every reboot (when PC returns to $F800D0).
    let mut d7_set_history: Vec<(u32, u32)> = Vec::new();
    let mut prev_d7 = m.cpu().regs.d[7];
    // Stage J: capture SSP-top value when PC reaches $F83560 (the
    // MOVE.L (A7)+, D7 instruction that loads D7 with the alert code).
    let mut alert_pop_history: Vec<(u32, u32, u32)> = Vec::new();
    // Stage J refinement: every time exc_vector transitions to Some(11)
    // (F-line trap), capture the CPU state that determines where we
    // jump next. Specifically: VBR (where the vector table starts),
    // overlay state (does $002C go to chip RAM or ROM?), the chip-RAM
    // backdoor value at $002C (what KS *wrote* there), and the value
    // the CPU would actually read via the normal memory path (which is
    // OVL-gated). If the two read paths disagree, OVL is interfering.
    // Also capture instr_start_pc (the F-line instruction) and the
    // *previous* PC (to confirm we trapped at the FPU probe site).
    let mut vec11_captures: Vec<(u32, u32, u32, bool, u32, u32)> = Vec::new();
    // Stage J: track every WRITE to chip[$002C]. Sample chip[$002C]
    // every tick via the backdoor; record (tick, pc, old, new) on
    // change. Cheap (one byte read per tick after the first), and
    // tells us exactly which instruction wrote the vector.
    let mut chip_002c_writes: Vec<(u64, u32, u32)> = Vec::new();
    let mut prev_chip_002c: u32 = ((m.read_chip_ram_byte(0x002C) as u32) << 24)
        | ((m.read_chip_ram_byte(0x002D) as u32) << 16)
        | ((m.read_chip_ram_byte(0x002E) as u32) << 8)
        | (m.read_chip_ram_byte(0x002F) as u32);
    // Stage J refinement #2: capture every transition from ROM (or
    // any PC inside $00F80000-$00FFFFFF) into the post-chip-RAM range
    // $00200000-$00BFFFFF. The vec11_captures show the second F-line
    // trap fires at PC=$002000F8 — open bus past 2MB chip RAM, which
    // reads as $FFFFFFFF and decodes as line-F. We need the ROM
    // instruction that initiated the jump there. Capture (tick, prev,
    // cur) for the first 30 such transitions, prioritising the FIRST
    // cycle (before any re-arm).
    let mut wild_jumps: Vec<(u64, u32, u32)> = Vec::new();
    // Stage J refinement #3: when the wild jump fires, dump full CPU
    // state. prev_pc=$F80C10 (middle of ORI.W #$0700, SR) → pc=$002000F8
    // is a one-tick jump that cannot be a normal instruction. Capture
    // (instr_start_pc, sr, exc_vector, ssp top-frame) to identify what
    // exception (if any) fires.
    let mut wild_jump_states: Vec<(u32, u16, Option<u8>, u32, u32, u32)> = Vec::new();
    // Stage J refinement #4: track LAST 5 PCs leading up to a wild
    // jump — a small circular buffer to see the path into the trap.
    let mut pc_history: Vec<u32> = Vec::with_capacity(6);
    // Stage J refinement #5: capture the SSP and longword at SSP each
    // time the RTS at $F80C0C is ABOUT to execute. The RTS pops 4
    // bytes; that long is the new PC. We expect either $002000F8 (the
    // wild-jump source) or a sane ROM address.
    let mut rts_f80c0c_captures: Vec<(u64, u32, u32, u32)> = Vec::new();
    // Stage J refinement #6: capture each RTS/JMP/JSR/RTE/BSR
    // instruction's pre-execution SSP and intended target by hooking
    // on the LAST PC visited before the wild-jump tick. Specifically
    // we want to identify what pushed $002000F8 onto the stack. Track
    // every PUSH (movement of SSP downward by 4) and capture the
    // pre-push SSP and the post-push value at SSP. Compare against
    // $002000F8 in particular.
    let mut prev_ssp: u32 = m.cpu().regs.ssp;
    let mut ssp_pushes_002000f8: Vec<(u64, u32, u32)> = Vec::new();
    // Stage J refinement #7: watch chip[$001FFFEA] (where the wild
    // RTS will pop from) and record every change with (tick, pc, ssp,
    // new_long).
    let mut chip_1fffea_changes: Vec<(u64, u32, u32, u32)> = Vec::new();
    let mut prev_1fffea: u32 = ((m.read_chip_ram_byte(0x001F_FFEA) as u32) << 24)
        | ((m.read_chip_ram_byte(0x001F_FFEB) as u32) << 16)
        | ((m.read_chip_ram_byte(0x001F_FFEC) as u32) << 8)
        | (m.read_chip_ram_byte(0x001F_FFED) as u32);
    // Stage K: capture the LAST 100 unique PCs visited before the
    // alert dispatcher at $F83558 is first reached. The path that
    // gets us there will reveal which routine pushed the alert code.
    // Use a circular buffer of PCs that excludes the alert-loop range
    // ($F80440-$F80460, $F83558+).
    let mut pre_alert_pcs: std::collections::VecDeque<u32> =
        std::collections::VecDeque::with_capacity(101);
    let mut pre_alert_captured: Option<Vec<u32>> = None;
    // Stage K: capture the FULL SSP frame at the moment of $F83558
    // entry (16 longs above SSP).
    let mut pre_alert_ssp_frame: Option<Vec<(u32, u32)>> = None;
    // Stage K: track every push of $0000039C (cycle-1 alert code) and
    // every push of $00000006 (cycle-2 alert code = AN_MemCorrupt).
    let mut alert_pushes: Vec<(u64, u32, u32, u32)> = Vec::new();
    // Stage K: track every entry to $F80B0E (vec 2/3 entry — group 0
    // bus/address error). Our exception counter only tracks group
    // 1/2; group 0 (bus/addr error) is invisible. Capture (tick,
    // prev_pc, instr_start_pc, ssp, top0, top1, top2) on each entry.
    let mut vec23_entries: Vec<(u64, u32, u32, u32, u32, u32, u32)> = Vec::new();
    // Stage K: track SR transitions out of supervisor mode (bit 13
    // cleared). Each entry to user mode is a candidate cause of the
    // SR=$0018 AE-frame mystery.
    let mut user_mode_transitions: Vec<(u64, u32, u32, u16)> = Vec::new();
    let mut prev_supervisor = m.cpu().regs.is_supervisor();
    // Stage K: track A5 changes. A5's value at the LVO -36 call
    // determines what (A5)'s callback does. If A5 = $F82956 (the
    // RTE-ending block), it explains the cascade.
    let mut a5_changes: Vec<(u64, u32, u32)> = Vec::new();
    let mut prev_a5 = m.cpu().regs.a[5];
    // Stage I: track CPU state at each of the 4 suspect validation
    // branches. When PC reaches the BNE/BMI instruction, capture
    // (PC, D0, D7, A6, ssp_top) so we can see which branch sends us
    // back to the reboot init.
    let mut validation_hits: Vec<(u32, u32, u32, u32, u32)> = Vec::new();
    let validation_pcs = [
        0x00F8_3598u32, // BNE.S after BTST #0, D0 (ExecBase alignment)
        0x00F8_35A0u32, // BNE.S after ADD.L ChkBase, D0
        0x00F8_35B2u32, // BNE.S after CMP.L (A7)+, D0 (memory test)
        0x00F8_35B6u32, // BMI.S after TST.L D7
    ];
    for f in 1..=frames_to_run {
        for _ in 0..PAL_FRAME_TICKS {
            m.tick();
            tick_counter = tick_counter.wrapping_add(1);
            let pc = m.cpu().regs.pc;
            unique_pcs.insert(pc);
            if tick_counter & 0x7F == 0 {
                *hot_pcs.entry(pc).or_insert(0) += 1;
            }
            if (0x00F8_0000..0x0100_0000).contains(&pc) {
                last_pc_in_rom = pc;
            } else if pc < 0x00F8_0000 {
                excursion_count += 1;
            }
            // Track entries into the byte-receive routine. Key by the
            // *previous* PC — the BSR instruction site that branched
            // here. Stack pointer reads are unreliable mid-tick.
            if pc == 0x00F8_3182 && prev_pc != 0x00F8_3182 {
                *byte_receive_entries.entry(prev_pc).or_insert(0) += 1;
            }
            // Track first-time entries into the Wack prologue range.
            let prev_in_wack = (wack_entry_lo..wack_entry_hi).contains(&prev_pc);
            let cur_in_wack = (wack_entry_lo..wack_entry_hi).contains(&pc);
            if cur_in_wack && !prev_in_wack && wack_entries.len() < 10 {
                let ssp = m.cpu().regs.ssp;
                let sp_top = m.read_long(ssp);
                let sp_next = m.read_long(ssp.wrapping_add(4));
                wack_entries.push((prev_pc, pc, sp_top, sp_next));
            }
            // Track first-time entries into DiagAlive ($F83616) too.
            if pc == 0x00F8_3616 && prev_pc != 0x00F8_3616 && diagalive_entries.len() < 10 {
                let ssp = m.cpu().regs.ssp;
                let sp_top = m.read_long(ssp);
                let sp_next = m.read_long(ssp.wrapping_add(4));
                let sp_next2 = m.read_long(ssp.wrapping_add(8));
                diagalive_entries.push((prev_pc, sp_top, sp_next, sp_next2));
            }
            // Track entries into the reboot-loop init function range
            // ($F835F0-$F83614). Use a range so we catch any entry
            // point KS uses to jump into the middle.
            let prev_in_range = (0x00F8_35F0..0x00F8_3614).contains(&prev_pc);
            let cur_in_range = (0x00F8_35F0..0x00F8_3614).contains(&pc);
            if cur_in_range && !prev_in_range && reboot_init_entries.len() < 10 {
                reboot_init_entries.push((prev_pc, pc));
            }
            // Stage I: snapshot register state when PC equals one of
            // the validation-branch addresses (catches the state right
            // before the branch condition is evaluated).
            if validation_pcs.contains(&pc)
                && !validation_pcs.contains(&prev_pc)
                && validation_hits.len() < 20
            {
                let cpu = m.cpu();
                validation_hits.push((
                    pc,
                    cpu.regs.d[0],
                    cpu.regs.d[7],
                    cpu.regs.a[5],
                    cpu.regs.ssp,
                ));
            }
            // Track re-entries via the $F80DB8 reset trampoline. The
            // last instruction of that trampoline is JMP (A0) which
            // lands at $F800D0 (the RESET pre-amble).
            if pc == 0x00F8_00D0 && prev_pc != 0x00F8_00D0 && reset_reentries.len() < 10 {
                reset_reentries.push(prev_pc);
            }
            // Detect D7 acquiring bit 31 (negative). Capture the PC
            // and the resulting D7 value. Only record up to 20.
            let cur_d7 = m.cpu().regs.d[7];
            if (prev_d7 & 0x8000_0000 == 0)
                && (cur_d7 & 0x8000_0000 != 0)
                && d7_set_history.len() < 20
            {
                d7_set_history.push((pc, cur_d7));
            }
            prev_d7 = cur_d7;
            // Stage J: capture stack top when PC reaches $F83560
            // (the MOVE.L (A7)+, D7 that loads the alert code).
            if pc == 0x00F8_3560 && prev_pc != 0x00F8_3560 && alert_pop_history.len() < 20 {
                let ssp = m.cpu().regs.ssp;
                let val = m.read_long(ssp);
                let val_next = m.read_long(ssp.wrapping_add(4));
                alert_pop_history.push((ssp, val, val_next));
            }
            // Stage J: detect transitions INTO the post-chip-RAM
            // window $00200000-$00BFFFFF. Must run BEFORE the
            // `prev_pc = pc` update so the edge is detectable.
            let prev_in_wild = (0x0020_0000..0x00C0_0000).contains(&prev_pc);
            let cur_in_wild = (0x0020_0000..0x00C0_0000).contains(&pc);
            if cur_in_wild && !prev_in_wild && wild_jumps.len() < 30 {
                wild_jumps.push((tick_counter, prev_pc, pc));
                // Snapshot CPU state at the moment of wild jump.
                let cpu = m.cpu();
                let ssp = cpu.regs.ssp;
                let top0 = m.read_long(ssp);
                let top1 = m.read_long(ssp.wrapping_add(4));
                wild_jump_states.push((
                    cpu.instr_start_pc,
                    cpu.regs.sr,
                    cpu.exc_vector,
                    ssp,
                    top0,
                    top1,
                ));
            }
            // Maintain a 5-deep PC history (only when PC actually
            // changed) to see the immediate path into the wild jump.
            if pc_history.last() != Some(&pc) {
                if pc_history.len() >= 5 {
                    pc_history.remove(0);
                }
                pc_history.push(pc);
            }
            // Stage K: maintain a 100-deep circular buffer of PCs
            // that *aren't* in the alert / blinker range. The moment
            // PC first reaches the alert dispatcher at $F83558,
            // freeze the buffer — it then shows the call path
            // leading into the alert.
            if pre_alert_captured.is_none() && pc != prev_pc {
                let is_alert_range = (0x00F8_3558..=0x00F8_3660).contains(&pc)
                    || (0x00F8_0440..=0x00F8_0460).contains(&pc);
                if pc == 0x00F8_3558 {
                    pre_alert_captured = Some(pre_alert_pcs.iter().copied().collect());
                    // Also snapshot the top 16 longs above SSP at the
                    // moment of alert entry.
                    let ssp = m.cpu().regs.ssp;
                    let mut frame = Vec::with_capacity(16);
                    for i in 0..16 {
                        let a = ssp.wrapping_add(i * 4);
                        frame.push((a, m.read_long(a)));
                    }
                    pre_alert_ssp_frame = Some(frame);
                } else if !is_alert_range {
                    if pre_alert_pcs.len() >= 100 {
                        pre_alert_pcs.pop_front();
                    }
                    pre_alert_pcs.push_back(pc);
                }
            }
            // Stage J: capture state when RTS at $F80C0C is about to
            // execute. instr_start_pc tracks the executing instruction;
            // the tick before this RTS completes its pop, we want to
            // know what's at SSP. Detect by edge: instr_start_pc
            // transitions to $F80C0C.
            let instr_pc_now = m.cpu().instr_start_pc;
            if instr_pc_now == 0x00F8_0C0C
                && rts_f80c0c_captures.len() < 20
                && rts_f80c0c_captures
                    .last()
                    .is_none_or(|(t, _, _, _)| tick_counter.wrapping_sub(*t) > 4)
            {
                let ssp = m.cpu().regs.ssp;
                let popped = m.read_long(ssp);
                let next = m.read_long(ssp.wrapping_add(4));
                rts_f80c0c_captures.push((tick_counter, ssp, popped, next));
            }
            // Stage J: detect pushes of $002000F8. A push moves SSP
            // down by 4 (or 2). When SSP decreases by exactly 4 AND
            // the new long at SSP is $002000F8, capture (tick, pc).
            let cur_ssp = m.cpu().regs.ssp;
            if cur_ssp.wrapping_add(4) == prev_ssp {
                let pushed = m.read_long(cur_ssp);
                if pushed == 0x0020_00F8 && ssp_pushes_002000f8.len() < 20 {
                    ssp_pushes_002000f8.push((tick_counter, pc, cur_ssp));
                }
            }
            // Stage K: detect pushes of cycle-1/2/3+ alert codes.
            if cur_ssp.wrapping_add(4) == prev_ssp && alert_pushes.len() < 30 {
                let pushed = m.read_long(cur_ssp);
                if pushed == 0x0000_039C || pushed == 0x0000_0006 || pushed == 0x0000_03FF {
                    alert_pushes.push((tick_counter, pc, cur_ssp, pushed));
                }
            }
            // Stage K: capture state on entry to vec 2/3 ($F80B0E).
            // This is the group-0 (bus error / address error) handler
            // entry. Our exception counter doesn't track group 0, so
            // this is the only way to spot bus-error firings.
            if pc == 0x00F8_0B0E && prev_pc != 0x00F8_0B0E && vec23_entries.len() < 20 {
                let cpu = m.cpu();
                let ssp = cur_ssp;
                let t0 = m.read_long(ssp);
                let t1 = m.read_long(ssp.wrapping_add(4));
                let t2 = m.read_long(ssp.wrapping_add(8));
                vec23_entries.push((tick_counter, prev_pc, cpu.instr_start_pc, ssp, t0, t1, t2));
            }
            // Stage K: track transitions OUT of supervisor mode.
            let cur_supervisor = m.cpu().regs.is_supervisor();
            if prev_supervisor && !cur_supervisor && user_mode_transitions.len() < 30 {
                let cpu = m.cpu();
                user_mode_transitions.push((tick_counter, pc, cpu.instr_start_pc, cpu.regs.sr));
            }
            prev_supervisor = cur_supervisor;
            // Stage K: track changes to A5.
            let cur_a5 = m.cpu().regs.a[5];
            if cur_a5 != prev_a5 && a5_changes.len() < 30 {
                a5_changes.push((tick_counter, pc, cur_a5));
            }
            prev_a5 = cur_a5;
            prev_ssp = cur_ssp;
            // Stage J: detect changes to chip[$001FFFEA]. Sample only
            // when an instruction boundary likely passed (pc changed)
            // to keep cost low. Stop tracking after tick 17M (after
            // first wild jump) to keep the captured log focused on
            // cycle 1 events.
            if pc != prev_pc && tick_counter < 17_000_000 {
                let cur_1fffea: u32 = ((m.read_chip_ram_byte(0x001F_FFEA) as u32) << 24)
                    | ((m.read_chip_ram_byte(0x001F_FFEB) as u32) << 16)
                    | ((m.read_chip_ram_byte(0x001F_FFEC) as u32) << 8)
                    | (m.read_chip_ram_byte(0x001F_FFED) as u32);
                if cur_1fffea != prev_1fffea && chip_1fffea_changes.len() < 60 {
                    chip_1fffea_changes.push((tick_counter, pc, m.cpu().regs.ssp, cur_1fffea));
                    prev_1fffea = cur_1fffea;
                }
            }
            prev_pc = pc;
            let ipl = m.cpu().regs.interrupt_mask();
            if ipl < min_ipl_seen {
                min_ipl_seen = ipl;
                if first_ipl_drop_frame.is_none() {
                    first_ipl_drop_frame = Some(f);
                }
            }
            if first_vbr_change_frame.is_none() && m.cpu().regs.vbr != 0 {
                first_vbr_change_frame = Some(f);
            }
            // Exception-vector edge detection.
            let cur_exc = m.cpu().exc_vector;
            if cur_exc != prev_exc
                && let Some(v) = cur_exc
            {
                *exc_counts.entry(v).or_insert(0) += 1;
                exc_first_pc.entry(v).or_insert(m.cpu().instr_start_pc);
                // Stage J: on vec 11 (F-line) fire, snapshot the
                // vector-table state. instr_start_pc is the F-line
                // opcode; vbr+11*4=$002C is where the CPU reads its
                // handler. Compare the OVL-gated read with the
                // chip-RAM backdoor: if they disagree, OVL is the
                // culprit; if they agree, KS's install at chip[$002C]
                // didn't stick (or was overwritten).
                if v == 11 && vec11_captures.len() < 12 {
                    let cpu = m.cpu();
                    let vbr = cpu.regs.vbr;
                    let instr_pc = cpu.instr_start_pc;
                    let vec_addr = vbr.wrapping_add(11 * 4);
                    let ovl = m.memory().overlay();
                    let cpu_path = m.read_long(vec_addr);
                    let chip_path = ((m.read_chip_ram_byte(vec_addr) as u32) << 24)
                        | ((m.read_chip_ram_byte(vec_addr.wrapping_add(1)) as u32) << 16)
                        | ((m.read_chip_ram_byte(vec_addr.wrapping_add(2)) as u32) << 8)
                        | (m.read_chip_ram_byte(vec_addr.wrapping_add(3)) as u32);
                    vec11_captures.push((instr_pc, vbr, cpu_path, ovl, chip_path, prev_pc));
                }
            }
            prev_exc = cur_exc;
            // Stage J: detect writes to chip[$002C]. Sample only when
            // PC changes (writes occur at instruction boundaries) to
            // avoid the per-tick 4-byte chip-RAM probe.
            if pc != prev_pc {
                let cur_chip_002c: u32 = ((m.read_chip_ram_byte(0x002C) as u32) << 24)
                    | ((m.read_chip_ram_byte(0x002D) as u32) << 16)
                    | ((m.read_chip_ram_byte(0x002E) as u32) << 8)
                    | (m.read_chip_ram_byte(0x002F) as u32);
                if cur_chip_002c != prev_chip_002c && chip_002c_writes.len() < 30 {
                    chip_002c_writes.push((tick_counter, pc, cur_chip_002c));
                }
                prev_chip_002c = cur_chip_002c;
            }
        }
        if f % checkpoint_every == 0 {
            let cpu = m.cpu();
            eprintln!(
                "  checkpoint frame {f:>4}:  PC=${:08X}  IPL={}  VBR=${:08X}  custom_writes={}  intena_writes={}",
                cpu.regs.pc,
                cpu.regs.interrupt_mask(),
                cpu.regs.vbr,
                m.debug_custom_write_log.len(),
                m.debug_intena_writes,
            );
            _last_checkpoint_pc = cpu.regs.pc;
        }
    }
    eprintln!(
        "milestones:  min IPL = {min_ipl_seen}  first IPL drop = {:?}  first VBR change = {:?}",
        first_ipl_drop_frame, first_vbr_change_frame
    );

    // Hot PCs — where is the CPU actually spending most of its time?
    let mut hot_sorted: Vec<_> = hot_pcs.iter().collect();
    hot_sorted.sort_by(|a, b| b.1.cmp(a.1));
    let total_samples: u64 = hot_sorted.iter().map(|(_, c)| **c).sum();
    eprintln!(
        "hot PCs (top 10, sampled every 128th tick, {} total samples):",
        total_samples
    );
    for (pc, count) in hot_sorted.iter().take(10) {
        let pct = (**count as f64 / total_samples as f64) * 100.0;
        eprintln!("  ${pc:08X}: {count} samples ({pct:.1}%)");
    }

    // Byte-receive call sites (sorted by frequency).
    let mut br_sorted: Vec<_> = byte_receive_entries.iter().collect();
    br_sorted.sort_by(|a, b| b.1.cmp(a.1));
    let total_br: u64 = byte_receive_entries.values().sum();
    eprintln!("byte-receive $F83182 entries (total {total_br}, keyed by BSR site PC):");
    for (caller_pc, count) in br_sorted.iter().take(10) {
        eprintln!("  from ${caller_pc:08X}: {count} entries");
    }

    // Check whether the FPU-probe handler installation at $F80C2C
    // ("MOVE.L A1, $0010.W") and $F80C30 ("MOVE.L A1, $002C.W") are
    // visited. If yes, KS does install the custom vec 4/11 handlers
    // before the FPU probe.
    eprintln!("FPU-probe handler installation visits:");
    for site in [
        0x00F8_0C2Cu32,
        0x00F8_0C30,
        0x00F8_0C28,
        0x00F8_0C20,
        0x00F8_0C9E,
        0x00F8_0CA0,
    ] {
        let visited = unique_pcs.contains(&site);
        eprintln!("  ${site:08X}: {}", if visited { "YES" } else { "no" });
    }

    eprintln!("$F83560 MOVE.L (A7)+, D7 alert-pop captures (SSP, val, val_next):");
    if alert_pop_history.is_empty() {
        eprintln!("  (none)");
    }
    for (ssp, val, val_next) in alert_pop_history.iter() {
        eprintln!("  SSP=${ssp:08X} -> popped=${val:08X}, next=${val_next:08X}");
    }

    // Stage J: vec 11 fire snapshots — CPU-path vs chip-RAM-path
    // disagreement reveals OVL; matching values reveal install failure.
    eprintln!("Vec 11 (F-line) fire snapshots (first 12):");
    if vec11_captures.is_empty() {
        eprintln!("  (no F-line traps fired)");
    }
    for (instr_pc, vbr, cpu_path, ovl, chip_path, prev) in vec11_captures.iter() {
        eprintln!(
            "  at instr_pc=${instr_pc:08X} (prev_pc=${prev:08X}) VBR=${vbr:08X}  \
             CPU-read[$2C]=${cpu_path:08X}  chip-backdoor[$2C]=${chip_path:08X}  OVL={ovl}"
        );
    }

    // Stage J: every ROM-to-post-chip-RAM transition. The first such
    // transition in cycle 1 is the ROM instruction that initiated the
    // wild jump to $002000F8.
    eprintln!("Wild jumps into $00200000-$00BFFFFF (first 30):");
    if wild_jumps.is_empty() {
        eprintln!("  (no jumps into post-chip-RAM window)");
    }
    for (tick, prev, pc) in wild_jumps.iter() {
        eprintln!("  tick={tick:>10}  prev_pc=${prev:08X}  -> ${pc:08X}");
    }
    eprintln!("Wild jump CPU state (first 30):");
    for (instr_pc, sr, exc, ssp, t0, t1) in wild_jump_states.iter() {
        eprintln!(
            "  instr_start_pc=${instr_pc:08X}  SR=${sr:04X}  exc={exc:?}  \
             SSP=${ssp:08X}  top=${t0:08X} next=${t1:08X}"
        );
    }
    eprintln!("Last 5 PCs at end of run:");
    for pc in pc_history.iter() {
        eprintln!("  ${pc:08X}");
    }
    eprintln!("Last 100 PCs before first reach of alert dispatcher $F83558:");
    if let Some(pcs) = &pre_alert_captured {
        if pcs.is_empty() {
            eprintln!("  (none captured)");
        }
        for pc in pcs {
            eprintln!("  ${pc:08X}");
        }
    } else {
        eprintln!("  (alert dispatcher $F83558 never reached)");
    }
    eprintln!("SSP frame at $F83558 entry (16 longs above SSP):");
    if let Some(frame) = &pre_alert_ssp_frame {
        for (addr, val) in frame {
            eprintln!("  @${addr:08X}: ${val:08X}");
        }
    } else {
        eprintln!("  (alert never reached)");
    }
    eprintln!("Pushes of $039C / $0006 / $03FF alert codes (first 30):");
    if alert_pushes.is_empty() {
        eprintln!("  (no direct push of these codes)");
    }
    for (tick, pc, ssp, val) in alert_pushes.iter() {
        eprintln!("  tick={tick:>10}  at PC=${pc:08X}  SSP=${ssp:08X}  pushed=${val:08X}");
    }
    eprintln!("Vec 2/3 entries (group-0 bus/addr error to $F80B0E) — first 20:");
    if vec23_entries.is_empty() {
        eprintln!("  (none — PC never reached $F80B0E from outside)");
    }
    for (tick, prev, ipc, ssp, t0, t1, t2) in vec23_entries.iter() {
        eprintln!(
            "  tick={tick:>10}  prev_pc=${prev:08X}  instr_pc=${ipc:08X}  SSP=${ssp:08X}  \
             top=${t0:08X} ${t1:08X} ${t2:08X}"
        );
    }
    eprintln!("Supervisor → user mode transitions (tick, pc, instr_pc, new SR) — first 30:");
    if user_mode_transitions.is_empty() {
        eprintln!("  (CPU never left supervisor mode)");
    }
    for (tick, pc, ipc, sr) in user_mode_transitions.iter() {
        eprintln!("  tick={tick:>10}  pc=${pc:08X}  instr_pc=${ipc:08X}  SR=${sr:04X}");
    }
    eprintln!("A5 changes (tick, PC, new A5) — first 30:");
    if a5_changes.is_empty() {
        eprintln!("  (A5 never changed)");
    }
    for (tick, pc, a5) in a5_changes.iter() {
        eprintln!("  tick={tick:>10}  pc=${pc:08X}  A5=${a5:08X}");
    }
    eprintln!("RTS at $F80C0C captures (tick, SSP, popped_long, next_long) — first 20:");
    if rts_f80c0c_captures.is_empty() {
        eprintln!("  (RTS at $F80C0C never executed)");
    }
    for (tick, ssp, popped, next) in rts_f80c0c_captures.iter() {
        eprintln!("  tick={tick:>10}  SSP=${ssp:08X}  popped=${popped:08X}  next=${next:08X}");
    }
    eprintln!("Pushes of $002000F8 (tick, PC, post-push SSP) — first 20:");
    if ssp_pushes_002000f8.is_empty() {
        eprintln!(
            "  (no push of $002000F8 detected — value may have been written by something other than a stack push, e.g. a MOVE.L (addr), -(SP) or stack misalignment)"
        );
    }
    for (tick, pc, ssp) in ssp_pushes_002000f8.iter() {
        eprintln!("  tick={tick:>10}  at PC=${pc:08X}  SSP=${ssp:08X}");
    }
    eprintln!("chip[$001FFFEA] long changes in cycle 1 (tick, PC, SSP, new_long) — first 60:");
    if chip_1fffea_changes.is_empty() {
        eprintln!("  (chip[$001FFFEA] never changed before tick 17M)");
    }
    for (tick, pc, ssp, val) in chip_1fffea_changes.iter() {
        eprintln!("  tick={tick:>10}  PC=${pc:08X}  SSP=${ssp:08X}  new=${val:08X}");
    }

    // Stage J: chip[$002C] write log — which instructions wrote the
    // vec-11 handler address into the vector table?
    eprintln!("chip[$002C] writes (first 30):");
    if chip_002c_writes.is_empty() {
        eprintln!("  (chip[$002C] never changed from its initial value)");
    }
    for (tick, pc, val) in chip_002c_writes.iter() {
        eprintln!("  tick={tick:>10}  PC=${pc:08X}  new_value=${val:08X}");
    }

    eprintln!("D7 bit-31 set events (PC, new D7) — first 20:");
    if d7_set_history.is_empty() {
        eprintln!("  (D7 never went negative — that's surprising)");
    }
    for (pc, d7) in d7_set_history.iter() {
        eprintln!("  at PC=${pc:08X}: D7 -> ${d7:08X}");
    }

    eprintln!("Validation-branch hits (PC, D0, D7, A5, SSP) — first 20:");
    for (pc, d0, d7, a5, ssp) in validation_hits.iter() {
        let name = match *pc {
            0x00F8_3598 => "BNE after BTST #0, D0 (ExecBase even?)",
            0x00F8_35A0 => "BNE after ADD.L ChkBase, D0 (sum -> -1?)",
            0x00F8_35B2 => "BNE after CMP.L (A7)+, D0 (memory test)",
            0x00F8_35B6 => "BMI after TST.L D7 (D7 negative?)",
            _ => "?",
        };
        eprintln!("  ${pc:08X} {name}: D0=${d0:08X} D7=${d7:08X} A5=${a5:08X} SSP=${ssp:08X}");
    }

    eprintln!("Reboot-loop init $F835F0 entries (first 10):");
    if reboot_init_entries.is_empty() {
        eprintln!("  (none)");
    }
    for (prev, _) in reboot_init_entries.iter() {
        eprintln!("  prev=${prev:08X} -> $F835F0");
    }

    // ExecBase validation: chip[$0004] = ExecBase pointer.
    // chip[ExecBase + $26] = ChkBase = ~ExecBase (bitwise complement).
    // If KS's validation routine fails this check, KS reboots.
    let mem2 = m.memory();
    let exec_base_hi = mem2.read_chip_ram_word(0x0004);
    let exec_base_lo = mem2.read_chip_ram_word(0x0006);
    let exec_base = (u32::from(exec_base_hi) << 16) | u32::from(exec_base_lo);
    eprintln!("ExecBase validation:");
    eprintln!("  chip[$0004] (ExecBase ptr) = ${exec_base:08X}");

    // Stage I: inspect the LVO -726 jump (exec/ColdReboot) which is
    // the suspected non-rebooting path. KS calls this expecting a
    // hard reset; if the LVO entry isn't a proper JMP $4EF9 + reset
    // implementation, the call returns and KS falls into the boot
    // self-test that subsequently reboots via the $F80DB8 trampoline.
    if (6..0x0020_0000).contains(&exec_base) {
        let lvo_addr = exec_base.wrapping_sub(0x2D6);
        let op_hi = mem2.read_chip_ram_word(lvo_addr);
        let tgt_hi = mem2.read_chip_ram_word(lvo_addr.wrapping_add(2));
        let tgt_lo = mem2.read_chip_ram_word(lvo_addr.wrapping_add(4));
        let target = (u32::from(tgt_hi) << 16) | u32::from(tgt_lo);
        eprintln!(
            "  LVO -726 ColdReboot @ chip[${lvo_addr:08X}]: opcode=${op_hi:04X} target=${target:08X}"
        );
        if op_hi == 0x4EF9 {
            eprintln!(
                "    JMP abs.L confirmed; target ${target:08X} {}",
                if (0x00F8_0000..0x0100_0000).contains(&target) {
                    "(ROM — likely valid)"
                } else {
                    "(NOT in ROM — likely broken)"
                }
            );
        } else {
            eprintln!("    Not a JMP abs.L — LVO table not installed!");
        }

        // Also check LVO -114 (Debug) for completeness.
        let dbg_addr = exec_base.wrapping_sub(0x72);
        let dbg_op = mem2.read_chip_ram_word(dbg_addr);
        let dbg_tgt_hi = mem2.read_chip_ram_word(dbg_addr.wrapping_add(2));
        let dbg_tgt_lo = mem2.read_chip_ram_word(dbg_addr.wrapping_add(4));
        let dbg_target = (u32::from(dbg_tgt_hi) << 16) | u32::from(dbg_tgt_lo);
        eprintln!(
            "  LVO -114 Debug @ chip[${dbg_addr:08X}]: opcode=${dbg_op:04X} target=${dbg_target:08X}"
        );
    }
    if exec_base < 0x0020_0000 {
        // ChkBase is at offset $26 from ExecBase.
        let ck_off = exec_base.wrapping_add(0x26);
        let ck_hi = mem2.read_chip_ram_word(ck_off);
        let ck_lo = mem2.read_chip_ram_word(ck_off.wrapping_add(2));
        let ck = (u32::from(ck_hi) << 16) | u32::from(ck_lo);
        let expected = !exec_base;
        eprintln!(
            "  chip[ExecBase+$26] (ChkBase) = ${ck:08X}  expected ~ExecBase = ${expected:08X}",
        );
        if ck == expected {
            eprintln!("  ChkBase OK — A6 + ChkBase = -1, validation would PASS");
        } else {
            eprintln!("  ChkBase MISMATCH — validation would FAIL (KS reboots)");
        }
    } else {
        eprintln!("  (ExecBase outside chip RAM — can't validate)");
    }

    eprintln!("$F800D0 (RESET re-entry) prev PCs (first 10):");
    if reset_reentries.is_empty() {
        eprintln!("  (none)");
    }
    for prev in reset_reentries.iter() {
        eprintln!("  prev=${prev:08X} -> $F800D0");
    }

    eprintln!("DiagAlive $F83616 entries (first 10):");
    if diagalive_entries.is_empty() {
        eprintln!("  (none)");
    }
    for (prev, sp0, sp4, sp8) in diagalive_entries.iter() {
        eprintln!("  prev=${prev:08X}  SSP[0]=${sp0:08X} SSP[4]=${sp4:08X} SSP[8]=${sp8:08X}");
    }

    eprintln!("Wack prologue entries (first 10, captured at PC transition):");
    if wack_entries.is_empty() {
        eprintln!("  (none — code never entered $F8325E-$F832B0)");
    }
    for (prev, entry, sp_top, sp_next) in wack_entries.iter() {
        eprintln!(
            "  prev=${prev:08X} -> entry=${entry:08X}  SSP[0]=${sp_top:08X} SSP[4]=${sp_next:08X}"
        );
    }

    // Which of the 14 known callers of exec/Debug (LVO -114) were
    // hit during the boot? That tells us which code path actually
    // invokes Debug() and causes the Wack entry.
    const DEBUG_CALLERS: &[u32] = &[
        0x00F8_3154,
        0x00F8_3A9E,
        0x00F8_3B44,
        0x00FC_388C,
        0x00FD_F4B0,
        0x00FF_6546,
        0x00FF_8E90,
        0x00FF_9BE4,
        0x00FF_ADCE,
        0x00FF_B1E6,
        0x00FF_CDA2,
        0x00FF_D28E,
        0x00FF_D6B4,
        0x00FF_DA30,
    ];
    eprintln!("exec/Debug callers visited during boot:");
    let mut any = false;
    for caller in DEBUG_CALLERS {
        if unique_pcs.contains(caller) {
            eprintln!("  ${caller:08X}: YES (visited)");
            any = true;
        }
    }
    if !any {
        eprintln!("  (none of the known Debug callers were visited)");
    }

    // Exception counts — if KS is hitting an illegal-instruction trap
    // or line-A/F trap and falling into a reset handler, these counts
    // will be high.
    let mut exc_sorted: Vec<_> = exc_counts.iter().collect();
    exc_sorted.sort_by(|a, b| b.1.cmp(a.1));
    eprintln!("exceptions taken (top 10):");
    if exc_sorted.is_empty() {
        eprintln!("  (none)");
    }
    for (vector, count) in exc_sorted.iter().take(10) {
        let first_pc = exc_first_pc.get(vector).copied().unwrap_or(0);
        eprintln!(
            "  vector {:>3} ({}): {count} taken, first at PC=${first_pc:08X}",
            vector,
            exception_vector_name(**vector)
        );
    }

    // Hottest custom-register writes — keyed by *offset* (the
    // `debug_custom_write_log` tuple stores PC at .1 and chipset
    // offset at .3, so group by .3).
    let mut writes_by_offset: std::collections::HashMap<u16, u64> =
        std::collections::HashMap::new();
    for entry in m.debug_custom_write_log.iter() {
        *writes_by_offset.entry(entry.3).or_insert(0) += 1;
    }
    let mut sorted: Vec<_> = writes_by_offset.into_iter().collect();
    sorted.sort_by_key(|item| std::cmp::Reverse(item.1));
    eprintln!("hottest custom register writes (top 5, by chipset offset):");
    for (offset, count) in sorted.iter().take(5) {
        eprintln!(
            "  $DFF{:03X} ({}): {count} writes",
            offset,
            custom_register_name(*offset)
        );
    }

    // Hottest custom-register reads — keyed by chipset offset.
    let mut reads_sorted: Vec<_> = m.debug_reg_read_counts.iter().collect();
    reads_sorted.sort_by(|a, b| b.1.cmp(a.1));
    eprintln!("hottest custom register reads (top 5, by chipset offset):");
    for (offset, count) in reads_sorted.iter().take(5) {
        eprintln!(
            "  $DFF{:03X} ({}): {count} reads",
            offset,
            custom_register_name(**offset)
        );
    }

    report_state(&format!("after {frames_to_run} frames"), &m, frames_to_run);
    eprintln!(
        "unique PCs visited: {}   last PC in ROM: ${:08X}   excursions out of ROM: {}",
        unique_pcs.len(),
        last_pc_in_rom,
        excursion_count
    );
    dump_code_at(&m, m.cpu().regs.pc, 8);

    // Chipset activity counters — proxy for "did the boot touch
    // hardware at all?"
    eprintln!(
        "chipset activity:  custom_write_log={}   intena_writes={}   reg_read_kinds={}",
        m.debug_custom_write_log.len(),
        m.debug_intena_writes,
        m.debug_reg_read_counts.len()
    );

    eprintln!(
        "PC delta from initial: ${:08X} -> ${:08X}  ({} unique addresses seen)",
        initial_pc,
        m.cpu().regs.pc,
        unique_pcs.len()
    );

    // Vector table inspection. KS sets up the 68k exception vector
    // table at low chip-RAM addresses ($00000000-$000003FF). If KS
    // has cleared OVL, the CPU reads these from RAM directly. If OVL
    // is still set, vectors are read from the ROM mirror.
    eprintln!("OVL state at end of run: {}", m.memory().overlay());
    eprintln!("Chip-RAM exception vector table after boot run:");
    let mem = m.memory();
    for vec in [0u32, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 24, 31] {
        let off = vec * 4;
        let hi = mem.read_chip_ram_word(off);
        let lo = mem.read_chip_ram_word(off + 2);
        let val = (u32::from(hi) << 16) | u32::from(lo);
        eprintln!("  vec {vec:>2} @ chip[${off:08X}]: ${val:08X}");
    }

    // Stage G: scan chip RAM for any longword that points anywhere
    // into the Wack-related ROM area $F83260-$F83660 (covers the
    // prologue, dispatcher, command handlers, and DiagAlive). We're
    // looking for the function pointer KS installed that ultimately
    // routes into Wack.
    let chip_size = mem.chip_ram_size() as u32;
    let stack_low = m.cpu().regs.ssp.saturating_sub(8);
    let stack_high = 0x0020_0000u32;
    // Scan chip RAM for the trampoline address $F831EA (and nearby).
    eprintln!("Chip-RAM scan for pointers to Wack trampoline $F831E8-$F831FE:");
    let mut hits_tr = 0;
    for off in (0..chip_size).step_by(2) {
        if off + 4 > chip_size {
            break;
        }
        let hi = mem.read_chip_ram_word(off);
        let lo = mem.read_chip_ram_word(off + 2);
        let val = (u32::from(hi) << 16) | u32::from(lo);
        if (0x00F8_31E8..=0x00F8_31FE).contains(&val) {
            eprintln!("  chip[${off:08X}] = ${val:08X}");
            hits_tr += 1;
            if hits_tr > 40 {
                break;
            }
        }
    }
    if hits_tr == 0 {
        eprintln!("  (none found)");
    }

    eprintln!("Chip-RAM scan for non-stack pointers into Wack area $F83260-$F83660:");
    let mut hits = 0;
    for off in (0..chip_size).step_by(2) {
        if off + 4 > chip_size {
            break;
        }
        // Skip the stack region — we want non-stack pointers (data
        // structures, function tables, etc.).
        if (stack_low..stack_high).contains(&off) {
            continue;
        }
        let hi = mem.read_chip_ram_word(off);
        let lo = mem.read_chip_ram_word(off + 2);
        let val = (u32::from(hi) << 16) | u32::from(lo);
        if (0x00F8_3260..=0x00F8_3660).contains(&val) {
            eprintln!("  chip[${off:08X}] = ${val:08X}");
            hits += 1;
            if hits > 60 {
                eprintln!("  (truncated)");
                break;
            }
        }
    }
    if hits == 0 {
        eprintln!("  (none found — Wack entry must be via fall-through, not pointer)");
    }

    // Stage G: dump the supervisor-stack call chain. Walking up from
    // SSP gives the sequence of return PCs back to whatever entered
    // the Wack code path. Each BSR pushes a return PC; each
    // RTS pops one. The deeper we walk, the older the call.
    let ssp = m.cpu().regs.ssp;
    eprintln!("Stack walk from SSP=${ssp:08X} ({} bytes):", 64);
    for i in (0..64).step_by(4) {
        let addr = ssp.wrapping_add(i);
        let val = m.read_long(addr);
        let annotation = if (0x00F8_0000..0x0100_0000).contains(&val) {
            "(ROM)"
        } else if val < 0x00200000 {
            "(chip RAM)"
        } else {
            "(?)"
        };
        eprintln!("  SSP+{i:>3} @ ${addr:08X}: ${val:08X} {annotation}");
    }
}

/// Human-readable name for a 68k exception vector.
fn exception_vector_name(vector: u8) -> &'static str {
    match vector {
        0 => "reset SSP",
        1 => "reset PC",
        2 => "bus error",
        3 => "address error",
        4 => "illegal instruction",
        5 => "divide by zero",
        6 => "CHK / CHK2",
        7 => "TRAPV / TRAPcc",
        8 => "privilege violation",
        9 => "trace",
        10 => "line A (Axxx)",
        11 => "line F (Fxxx)",
        14 => "format error (68010+ RTE)",
        24 => "spurious interrupt",
        25..=31 => "autovector IRQ",
        32..=47 => "TRAP #n",
        _ => "user/MFP/other",
    }
}

/// Human-readable name for a chipset register offset (the names KS
/// authors used in the Hardware Reference Manual).
fn custom_register_name(offset: u16) -> &'static str {
    match offset {
        0x002 => "DMACONR",
        0x004 => "VPOSR",
        0x006 => "VHPOSR",
        0x00A => "JOY0DAT",
        0x00C => "JOY1DAT",
        0x010 => "ADKCONR",
        0x012 => "POT0DAT",
        0x014 => "POT1DAT",
        0x016 => "POTGOR",
        0x018 => "SERDATR",
        0x01A => "DSKBYTR",
        0x01C => "INTENAR",
        0x01E => "INTREQR",
        0x07E => "DSKSYNC",
        0x09A => "INTENA",
        0x09C => "INTREQ",
        0x09E => "ADKCON",
        0x180 => "COLOR00",
        _ => "?",
    }
}
