//! Lock in the M12-step-1+2 progression: after CIA-A TOD was wired
//! to /VSYNC (step 1) and the copper COPJMP1/2 strobes started
//! firing from MOVE instructions (step 2), Kickstart 1.3 gets past
//! the PAL/NTSC probe, runs through Exec + library init, and settles
//! with exec.library running the insert-disk animation while
//! trackdisk.device + input.device wait on signals.
//!
//! Step 2 note: once COPJMP2 strobes land, the copper chains from
//! COP1LC into whatever COP2LC points to. KS 1.3 settles the copper on
//! a valid display list (COP2LC = `\$00B888` on this boot) and renders
//! the white "insert-disk" screen (COLOR00 `\$0FFF`, BPLCON0 `\$0302`).
//!
//! **Steady state (verified 2026-06-12, #30 Phase 2).** Both RAM
//! configs boot WB 1.3 to an *identical, healthy* insert-disk screen —
//! framebuffer ≈430k/442k non-black pixels over 4 clean colours (black,
//! `\$7777CC`, `\$BBBBBB`, `\$FFFFFF`); copper running a real list, not
//! `stopped`, COLOR00/BPLCON0 stable. (An earlier note here described a
//! "COP2LC=ExecBase corruption" stomping the display — that is stale:
//! the ROM only writes ExecBase to COP2LC *transiently* (~frame 115),
//! then overwrites it with the real list; the display is never
//! corrupted under the current chipset.)
//!
//! Once booted, the CPU spends each 50 Hz frame mostly in the Exec idle
//! loop (`\$FC0700..\$FC0740`, user mode, IPL 0), dipping into the VBL /
//! insert-disk animation handler (`\$FE9xxx` in ROM + the task code in
//! slow RAM) and returning. Sampling the PC at a *single* instant is
//! therefore phase-fragile — which phase a fixed frame count lands in
//! moved when #30 installed the hardware-correct DMA slot timing. So
//! the test advances until the CPU is actually in the idle loop before
//! snapshotting the steady-state invariants:
//!   - user mode, IPL mask 0, DMACON `\$02D0`, INTENA `\$602C`,
//!     DispCount ≥ 100, TaskReady empty, exec.library = ThisTask (RUN),
//!     TaskWait = {trackdisk.device, input.device}.
//!
//! These are stable functional invariants (Exec task topology + a
//! reachable idle loop), robust to the per-CCK-bus, unified-driver, and
//! slot-allocation refactors. When a future change breaks one, this
//! test catches it before the regression cascades.

use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};
use std::path::PathBuf;

// ExecBase field offsets (exec/execbase.h).
const EXEC_THIS_TASK: u32 = 276;
const EXEC_DISP_COUNT: u32 = 284;
const EXEC_TASK_READY: u32 = 406;
const EXEC_TASK_WAIT: u32 = 420;

// Task struct field offsets (exec/tasks.h).
const TASK_LN_SUCC: u32 = 0;
const TASK_LN_NAME: u32 = 10;
const TASK_STATE: u32 = 15;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        eprintln!("skipping: Kickstart 1.3 ROM missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

fn read_long(amiga: &AmigaOcs, addr: u32) -> u32 {
    amiga.read_long(addr)
}

fn read_byte(amiga: &AmigaOcs, addr: u32) -> u8 {
    (amiga.read_word(addr & !1) >> (if addr & 1 == 0 { 8 } else { 0 })) as u8
}

fn read_cstring(amiga: &AmigaOcs, addr: u32, max: u32) -> String {
    let mut s = String::new();
    for i in 0..max {
        let b = read_byte(amiga, addr.wrapping_add(i));
        if b == 0 {
            break;
        }
        if b.is_ascii() && !b.is_ascii_control() {
            s.push(b as char);
        }
    }
    s
}

fn walk_task_names(amiga: &AmigaOcs, list_addr: u32) -> Vec<String> {
    let head = read_long(amiga, list_addr);
    let tail_sentinel = list_addr.wrapping_add(4);
    let mut names = Vec::new();
    let mut node = head;
    for _ in 0..8 {
        if node == 0 || node == tail_sentinel {
            break;
        }
        let name_ptr = read_long(amiga, node.wrapping_add(TASK_LN_NAME));
        names.push(read_cstring(amiga, name_ptr, 32));
        node = read_long(amiga, node.wrapping_add(TASK_LN_SUCC));
    }
    names
}

fn is_user_idle(amiga: &AmigaOcs) -> bool {
    let pc = amiga.cpu().regs.pc;
    let sr = amiga.cpu().regs.sr;
    (0x00FC_0700..=0x00FC_0740).contains(&pc) && (sr & 0x2700) == 0
}

fn advance_to_user_idle(amiga: &mut AmigaOcs) -> bool {
    let sub_frame = (PAL_FRAME_TICKS / 8).max(1);
    for _ in 0..160 {
        if is_user_idle(amiga) {
            return true;
        }
        for _ in 0..sub_frame {
            amiga.tick();
        }
    }
    is_user_idle(amiga)
}

/// Task #180 — cross-cutting boot scenario: Kickstart 1.3 reaches
/// the post-keyboard steady-state idle.
///
/// With CIA + Paula + Agnus + Blitter + Denise + Floppy + Keyboard
/// all wired, Kickstart 1.3 runs Exec init, spawns the standard
/// task set, and settles into Intuition's insert-disk animation.
/// Before the incremental blitter (#31), a DMACONR byte-read bug
/// made `btst.b #6, $DFF002` (BBUSY) always read 1, so the
/// animation task spun forever in graphics.library WAITBLIT at
/// `$FC5A6C`. With the blitter now clearing BBUSY, KS 1.3 exits
/// WAITBLIT and reaches the Exec idle loop at `$FC0722`, dipping into
/// the VBL animation handler each frame — the steady state this test
/// locks in.
#[test]
fn boot_reaches_insert_disk_idle_with_keyboard_live() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);

    // 300 frames is well past the point where the boot settles.
    for _ in 0..(300 * PAL_FRAME_TICKS) {
        amiga.tick();
    }

    // ── CPU: reach the user-mode Exec idle loop ─────────────
    // KS 1.3 alternates each frame between the user-mode Exec idle loop
    // ($FC0700..$FC0740, IPL 0) and the supervisor VBL / insert-disk
    // animation handler. A single-instant snapshot is phase-fragile —
    // the hardware-correct #30 slot timing changed which phase a fixed
    // frame count lands in, and a frame-aligned sample is biased toward
    // the VBL window (supervisor). The boot itself is unchanged: the
    // user-mode idle state is hit ~6 of every 48 sub-frame samples.
    // Step sub-frame until the CPU is in the idle loop in USER mode with
    // IPL 0 — the genuine settled state — then snapshot the invariants.
    let reached_idle = advance_to_user_idle(&mut amiga);
    let sr = amiga.cpu().regs.sr;
    assert!(
        reached_idle,
        "CPU should settle in the user-mode Exec idle loop near \\$FC0722 \
         (IPL 0); last PC=\\${:08X} SR=\\${sr:04X}",
        amiga.cpu().regs.pc
    );

    // Confirm the settled state explicitly (true by construction —
    // documents the invariant the search locked onto).
    assert_eq!(
        sr & 0x0700,
        0x0000,
        "IPL mask must be 0; got SR=\\${sr:04X}"
    );
    assert_eq!(
        sr & 0x2000,
        0x0000,
        "CPU must be in user mode; got SR=\\${sr:04X}"
    );

    // ── Chipset: same steady-state as pre-keyboard ──────────
    assert_eq!(
        amiga.dmacon() & 0x02FF,
        0x02D0,
        "DMACON should have DMAEN + COPEN + BLITEN + DSKEN, BPLEN off"
    );
    assert_eq!(
        amiga.intena() & 0x7FFF,
        0x602C,
        "INTENA should be \\$602C = master + EXTER + VERTB + PORTS + SOFT"
    );

    // ── ExecBase ────────────────────────────────────────────
    let exec_base = read_long(&amiga, 0x00000004);
    assert_ne!(exec_base, 0, "ExecBase must be initialised");
    let disp_count = read_long(&amiga, exec_base.wrapping_add(EXEC_DISP_COUNT));
    assert!(
        disp_count >= 100,
        "Exec should have done ≥ 100 task dispatches; got {disp_count}"
    );

    // ── TaskReady empty, ThisTask running ───────────────────
    let ready_addr = exec_base.wrapping_add(EXEC_TASK_READY);
    let ready_head = read_long(&amiga, ready_addr);
    let ready_tail = ready_addr.wrapping_add(4);
    assert_eq!(
        ready_head, ready_tail,
        "TaskReady should be empty — all runnable work is handled"
    );

    let this_task = read_long(&amiga, exec_base.wrapping_add(EXEC_THIS_TASK));
    assert_ne!(this_task, 0, "ThisTask must be set");
    let this_state = read_byte(&amiga, this_task.wrapping_add(TASK_STATE));
    assert_eq!(this_state, 2, "ThisTask should be in state RUN (2)");

    // ── TaskWait: trackdisk + input still blocked ───────────
    // Keyboard liveness doesn't move input.device off TaskWait —
    // input.device still waits on the port-signal bitmask; the SP
    // IRQ only dispatches from inside that wait. trackdisk.device
    // similarly waits on its MsgPort. exec.library remains the
    // ThisTask running the animation.
    let wait_addr = exec_base.wrapping_add(EXEC_TASK_WAIT);
    let names = walk_task_names(&amiga, wait_addr);
    assert!(
        names.iter().any(|n| n == "trackdisk.device"),
        "trackdisk.device should be in TaskWait (got {names:?})"
    );
    assert!(
        names.iter().any(|n| n == "input.device"),
        "input.device should be in TaskWait (got {names:?})"
    );
    assert!(
        !names.iter().any(|n| n == "exec.library"),
        "exec.library should NOT be in TaskWait — it is the running task (got {names:?})"
    );

    // ── Keyboard: power-up sequence has settled ─────────────
    // The keyboard controller needs at least the $FD + $FE pair
    // handshaked by the host. By 300 frames (~6 s wall-clock)
    // that pair is long done; bytes_sent counts both plus any
    // timeout-retransmit.
    assert!(
        amiga.keyboard().bytes_sent >= 2,
        "keyboard should have emitted at least the \\$FD + \\$FE power-up pair; got {}",
        amiga.keyboard().bytes_sent
    );
}

/// Companion assertion: slow-RAM and chip-only converge at the task
/// level even though their idle PCs and display-register state differ.
///
/// Task #96 closed the CDANG protection (the HRM's "dangerous MOVE
/// halts copper" rule), so chip-only no longer deadlocks on romboot's
/// TD_CHANGESTATE DoIO. Both configs complete boot, render the same
/// healthy insert-disk screen, and idle with the same Exec task
/// topology. The exact PC at any instant depends on the idle-loop ↔
/// VBL-handler phase (see the sibling test), so this test compares the
/// configs only at the CPU-mode + Exec-task level, which is stable.
#[test]
fn chip_only_and_slow_ram_converge_at_task_level() {
    let Some(rom) = load_kickstart() else { return };
    let mut slow = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    let mut chip_only = AmigaOcs::new(rom);

    for _ in 0..(300 * PAL_FRAME_TICKS) {
        slow.tick();
        chip_only.tick();
    }

    // A frame-boundary sample can land inside the VBL handler. Advance
    // each configuration independently until its user-mode Exec idle
    // loop is observable, then compare the stable task-level state.
    for (label, a) in [("slow-RAM", &mut slow), ("chip-only", &mut chip_only)] {
        assert!(
            advance_to_user_idle(a),
            "{label} should reach the user-mode Exec idle loop; \
             last PC=\\${:08X} SR=\\${:04X}",
            a.cpu().regs.pc,
            a.cpu().regs.sr
        );
        let pc = a.cpu().regs.pc;
        assert!(
            (0x00FC_0700..=0x00FC_0740).contains(&pc),
            "{label} should idle in the KS ROM; got PC=\\${pc:08X}"
        );
        let sr = a.cpu().regs.sr;
        assert_eq!(
            sr & 0x0700,
            0,
            "{label} IPL mask must be 0; got SR=\\${sr:04X}"
        );
    }

    // Same running task set — exec.library is ThisTask (RUN) in both;
    // trackdisk + input.device are both waiting.
    let slow_exec = read_long(&slow, 0x00000004);
    let chip_exec = read_long(&chip_only, 0x00000004);
    let mut slow_wait = walk_task_names(&slow, slow_exec.wrapping_add(EXEC_TASK_WAIT));
    let mut chip_wait = walk_task_names(&chip_only, chip_exec.wrapping_add(EXEC_TASK_WAIT));
    slow_wait.sort();
    chip_wait.sort();
    assert_eq!(
        slow_wait, chip_wait,
        "both configs should have the same TaskWait set"
    );
    assert!(
        !slow_wait.iter().any(|n| n == "exec.library"),
        "exec.library should be RUN (not WAIT) in both configs"
    );
}
