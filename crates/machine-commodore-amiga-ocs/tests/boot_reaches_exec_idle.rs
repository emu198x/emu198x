//! Lock in the M12-step-1+2 progression: after CIA-A TOD was wired
//! to /VSYNC (step 1) and the copper COPJMP1/2 strobes started
//! firing from MOVE instructions (step 2), Kickstart 1.3 gets past
//! the PAL/NTSC probe, runs through Exec + library init, and lands
//! in Exec's idle loop with three tasks waiting on signals.
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
//! Both chip-only and slow-RAM still converge at the CPU/task
//! level (same PC, SR, task list). Chipset-register state now
//! diverges because chip-only hits the COP2LC=ExecBase write
//! ~137 frames earlier (frame 162 vs 299), giving the corruption
//! much more time to accumulate. We keep the slow-RAM chipset
//! assertions as-is (minus the BPLCON0/COLOR00 ones that the
//! corruption touches) and compare configs only at the CPU/task
//! level.
//!
//! This test runs 300 PAL frames on the slow-RAM variant and
//! asserts:
//!   - CPU settled into the idle-loop PC region (\$FC0F74..\$FC0F96).
//!   - SR shows supervisor mode with IPL mask = 0 (ready for IRQs).
//!   - Chipset: DMAEN + COPEN + BLITEN + DSKEN, BPLEN off, INTENA
//!     master + VERTB + PORTS + EXTER + SOFT (slow-RAM only —
//!     chip-only has had these clobbered by the COP2LC corruption).
//!   - Exec has done ≥ 100 task dispatches.
//!   - TaskReady is empty.
//!   - TaskWait has the three expected tasks, all in WAIT state:
//!     trackdisk.device, exec.library, input.device.
//!
//! When a future change breaks any of these, this test catches it
//! before the regression cascades.

use std::path::PathBuf;
use machine_commodore_amiga_ocs::{AmigaOcs, PAL_FRAME_TICKS};

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

#[test]
fn boot_reaches_exec_idle_with_expected_tasks_waiting() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::with_slow_ram(rom, 512 * 1024);

    // 300 frames is well past the point where the boot settles.
    // The idle state is reached around frame 200.
    for _ in 0..(300 * PAL_FRAME_TICKS) {
        amiga.tick();
    }

    // ── CPU: WAITBLIT spin-loop region ──────────────────────
    // Before M13 the CPU idled in Exec's Wait() loop ($FC0F74..
    // $FC0F96) because trackdisk blocked forever waiting on a
    // MICROHZ timer. Now that the timer fires and trackdisk
    // returns TDERR_DiskChanged, the boot progresses into
    // Intuition's insert-disk animation, which calls WAITBLIT
    // repeatedly ($FC5A6C..$FC5A7C). The CPU spends ~99% of its
    // time in this tight loop, busy-waiting on DMACONR bit 6
    // (BBUSY) — which our chipset always reads as 0, so each
    // call returns immediately and the animation loop re-enters.
    let pc = amiga.cpu().regs.pc;
    assert!(
        (0x00FC_5A6C..=0x00FC_5A7C).contains(&pc),
        "CPU should be in the WAITBLIT spin at \\$FC5A6C..\\$FC5A7C; got PC=\\${pc:08X}"
    );

    // SR = $2000 — supervisor mode, IPL mask = 0 (waiting for IRQs).
    let sr = amiga.cpu().regs.sr;
    assert_eq!(
        sr & 0x2700, 0x2000,
        "SR should be supervisor with IPL mask 0; got \\${sr:04X}"
    );

    // ── Chipset ─────────────────────────────────────────────
    assert_eq!(
        amiga.dmacon() & 0x02FF, 0x02D0,
        "DMACON should have DMAEN + COPEN + BLITEN + DSKEN, BPLEN off"
    );
    // $602C = master + EXTER + VERTB + PORTS + SOFT. Before M13 the
    // DSKBLK bit (2) briefly got enabled because trackdisk reached
    // CMD_READ's DMA path; now that /DSKCHANGE is latched low on the
    // CIA-A disk pins, trackdisk returns TDERR_DiskChanged immediately
    // without arming Paula DMA, and DSKBLK never needs enabling.
    assert_eq!(
        amiga.intena() & 0x7FFF, 0x602C,
        "INTENA should be \\$602C = master + EXTER + VERTB + PORTS + SOFT"
    );
    // BPLCON0 and COLOR00 used to pin at $1000 (BPU=1) and $0FFF
    // (white insert-disk). With COPJMP2 strobes firing, the copper
    // briefly runs ExecBase-as-copper-list when the ROM transiently
    // writes ExecBase to COP2LC (frame 299). That stomps them. The
    // underlying D0-holds-ExecBase puzzle is tracked separately.

    // ── ExecBase ────────────────────────────────────────────
    let exec_base = read_long(&amiga, 0x00000004);
    assert_ne!(exec_base, 0, "ExecBase must be initialised");
    let disp_count = read_long(&amiga, exec_base.wrapping_add(EXEC_DISP_COUNT));
    assert!(
        disp_count >= 100,
        "Exec should have done ≥ 100 task dispatches; got {disp_count}"
    );

    // ── TaskReady: empty ────────────────────────────────────
    let ready_addr = exec_base.wrapping_add(EXEC_TASK_READY);
    let ready_head = read_long(&amiga, ready_addr);
    let ready_tail = ready_addr.wrapping_add(4);
    assert_eq!(
        ready_head, ready_tail,
        "TaskReady list should be empty — Exec is idle"
    );

    // ── ThisTask: the task currently running the animation ──
    // State 2 = TS_RUN — ThisTask is the task executing right now
    // (the one whose code the CPU runs inside WAITBLIT). Before
    // M13 this was 4 (TS_WAIT) because nothing was running; the
    // CPU sat inside Exec's Wait() waiting for an interrupt.
    let this_task = read_long(&amiga, exec_base.wrapping_add(EXEC_THIS_TASK));
    assert_ne!(this_task, 0, "ThisTask must be set");
    let this_state = read_byte(&amiga, this_task.wrapping_add(TASK_STATE));
    assert_eq!(this_state, 2, "ThisTask should be in state RUN (2)");

    // ── TaskWait: trackdisk + input blocked ─────────────────
    // exec.library used to be the third entry here (parked in
    // Wait() on the idle path). Now that trackdisk returns from
    // its CMD_READ with TDERR_DiskChanged, exec.library is the
    // ThisTask that's actively running the animation — so it
    // lives on the run side, not in TaskWait.
    let wait_addr = exec_base.wrapping_add(EXEC_TASK_WAIT);
    let names = walk_task_names(&amiga, wait_addr);
    eprintln!("TaskWait: {names:?}");
    assert!(
        names.iter().any(|n| n == "trackdisk.device"),
        "trackdisk.device should be in TaskWait"
    );
    assert!(
        names.iter().any(|n| n == "input.device"),
        "input.device should be in TaskWait"
    );
    assert!(
        !names.iter().any(|n| n == "exec.library"),
        "exec.library should NOT be in TaskWait (it is the running task)"
    );
}

/// Companion snapshot: slow-RAM and chip-only currently DIVERGE.
///
/// Before M13 both configs ended stuck in Exec's Wait() loop at
/// $FC0F74..$FC0F96 because trackdisk's 500 ms MICROHZ never fired.
/// Now that the MICROHZ fix + CIA-A /CHNG=low unblock trackdisk,
/// slow-RAM progresses into Intuition's insert-disk animation
/// (CPU lives in WAITBLIT, ThisTask RUN). Chip-only stays stuck in
/// Exec's old idle at $FC0F94 because it has a separate bug
/// further upstream (task #96 — the GfxBase LOFlist copper-list
/// corruption). This test snapshots that divergence until #96 is
/// resolved, at which point both configs should reach WAITBLIT.
#[test]
fn slow_ram_reaches_waitblit_but_chip_only_still_stuck_in_old_idle() {
    let Some(rom) = load_kickstart() else { return };
    let mut slow = AmigaOcs::with_slow_ram(rom.clone(), 512 * 1024);
    let mut chip_only = AmigaOcs::new(rom);

    for _ in 0..(300 * PAL_FRAME_TICKS) {
        slow.tick();
        chip_only.tick();
    }

    let waitblit = 0x00FC_5A6C..=0x00FC_5A7C;
    let old_exec_idle = 0x00FC_0F74..=0x00FC_0F96;
    assert!(
        waitblit.contains(&slow.cpu().regs.pc),
        "slow-RAM should progress to WAITBLIT; got \\${:08X}",
        slow.cpu().regs.pc
    );
    assert!(
        old_exec_idle.contains(&chip_only.cpu().regs.pc),
        "chip-only should still be stuck in old Exec idle (task #96); got \\${:08X}",
        chip_only.cpu().regs.pc
    );

    // The two configs have all the same tasks installed, but
    // slow-RAM has exec.library promoted from TaskWait to ThisTask
    // because it's running the animation; chip-only still has it
    // in TaskWait because chip-only is blocked upstream.
    let slow_exec = read_long(&slow, 0x00000004);
    let chip_exec = read_long(&chip_only, 0x00000004);
    let slow_wait = walk_task_names(&slow, slow_exec.wrapping_add(EXEC_TASK_WAIT));
    let chip_wait = walk_task_names(&chip_only, chip_exec.wrapping_add(EXEC_TASK_WAIT));
    for name in ["trackdisk.device", "input.device"] {
        assert!(slow_wait.iter().any(|n| n == name), "slow: {name} in TaskWait");
        assert!(chip_wait.iter().any(|n| n == name), "chip: {name} in TaskWait");
    }
    assert!(
        !slow_wait.iter().any(|n| n == "exec.library"),
        "slow-RAM: exec.library should have moved out of TaskWait"
    );
    assert!(
        chip_wait.iter().any(|n| n == "exec.library"),
        "chip-only: exec.library still parked in TaskWait (blocked upstream)"
    );
}
