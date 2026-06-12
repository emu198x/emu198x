//! Lock in the M12-step-1+2 progression: after CIA-A TOD was wired
//! to /VSYNC (step 1) and the copper COPJMP1/2 strobes started
//! firing from MOVE instructions (step 2), Kickstart 1.3 gets past
//! the PAL/NTSC probe, runs through Exec + library init, and settles
//! with exec.library running the insert-disk animation while
//! trackdisk.device + input.device wait on signals.
//!
//! Step 2 note: once COPJMP2 strobes land, the copper chains from
//! COP1LC into whatever COP2LC points to. On this boot, the ROM
//! briefly writes ExecBase into COP2LC near the end of frame 299
//! (`MOVE.L D0, \$84(A0)` at \$FC6D6C with D0 transiently holding
//! ExecBase). The copper then executes ExecBase's bytes as copper
//! instructions and stomps BPLCON0 / COLOR00 / etc. That's why the
//! white "insert-disk" screen no longer pins at \$0FFF — see the
//! follow-up task for the underlying D0-holds-ExecBase puzzle.
//!
//! Chip-only and slow-RAM converge at the Exec *task* level (same
//! task topology), but no longer on the same PC: chip-only hits the
//! COP2LC=ExecBase write ~137 frames earlier (frame 162 vs 299), so
//! its display registers are corrupted further and it idles deeper in
//! the screen-setup path. We keep the slow-RAM chipset assertions
//! (minus the BPLCON0/COLOR00 ones the corruption touches) and compare
//! the two configs only at the CPU-mode + task level — see the
//! re-baseline note below.
//!
//! **Re-baselined 2026-06-12 (#467).** The incremental blitter (#31) +
//! the DMACONR byte-read fix (#32) retired the old "CPU stuck in
//! WAITBLIT forever" state these tests used to pin. KS 1.3 now exits
//! the WAITBLIT spin and settles in the Exec idle loop. Measured
//! post-blitter steady state at 300 frames:
//!   - slow-RAM: PC `\$FC0722`, chip-only: PC `\$FE9C54` (the two
//!     configs settle at *different* idle PCs — chip-only hits the
//!     COP2LC=ExecBase corruption ~137 frames earlier, so it idles
//!     deeper in the screen-setup path; they still converge at the
//!     task level).
//!   - both: user mode, IPL mask 0, DMACON `\$02D0`, INTENA `\$602C`,
//!     DispCount ≥ 100, TaskReady empty, exec.library = ThisTask (RUN),
//!     TaskWait = {trackdisk.device, input.device}.
//!
//! These are stable functional invariants (ROM-determined idle PCs +
//! Exec task topology), so they survive the per-CCK-bus and unified-
//! driver refactors. When a future change breaks one, this test
//! catches it before the regression cascades.

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
/// WAITBLIT and settles in the Exec idle loop at `$FC0722` — the
/// steady state this test now locks in (#467).
#[test]
fn boot_reaches_insert_disk_idle_with_keyboard_live() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);

    // 300 frames is well past the point where the boot settles.
    for _ in 0..(300 * PAL_FRAME_TICKS) {
        amiga.tick();
    }

    // ── CPU: Exec idle loop (post-blitter, #467) ────────────
    // KS 1.3 no longer spins in WAITBLIT; slow-RAM settles in the
    // Exec idle region around $FC0722. A small range tolerates which
    // idle-loop instruction the 300-frame boundary lands on.
    let pc = amiga.cpu().regs.pc;
    assert!(
        (0x00FC_0700..=0x00FC_0740).contains(&pc),
        "CPU should be in the Exec idle loop near \\$FC0722; got PC=\\${pc:08X}"
    );

    // Keyboard-live regime: the animation runs as a user-mode task.
    // IPL mask = 0 — the CPU is ready to take interrupts.
    let sr = amiga.cpu().regs.sr;
    assert_eq!(
        sr & 0x0700,
        0x0000,
        "IPL mask must be 0 (taking interrupts); got SR=\\${sr:04X}"
    );
    assert_eq!(
        sr & 0x2000,
        0x0000,
        "CPU should be in user mode after keyboard made input.device dispatchable; got SR=\\${sr:04X}"
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
/// TD_CHANGESTATE DoIO. Both configs now complete boot and idle with
/// the same Exec task topology. They do NOT land on the same PC: the
/// chip-only config hits the COP2LC=ExecBase corruption ~137 frames
/// earlier (frame 162 vs 299), so it idles deeper in the screen-setup
/// path (`$FE9C54`) than slow-RAM (`$FC0722`). Display registers
/// (BPLCON0/COLOR00) diverge for the same reason — so this test
/// compares only the CPU-mode + Exec-task invariants, which match.
#[test]
fn chip_only_and_slow_ram_converge_at_task_level() {
    let Some(rom) = load_kickstart() else { return };
    let mut slow = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    let mut chip_only = AmigaOcs::new(rom);

    for _ in 0..(300 * PAL_FRAME_TICKS) {
        slow.tick();
        chip_only.tick();
    }

    // Both settle in the Kickstart ROM (executing the idle/setup path),
    // in user mode with interrupts unmasked — not crashed, not spinning
    // in supervisor with IPL raised.
    for (label, a) in [("slow-RAM", &slow), ("chip-only", &chip_only)] {
        let pc = a.cpu().regs.pc;
        assert!(
            (0x00F8_0000..=0x00FF_FFFF).contains(&pc),
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
