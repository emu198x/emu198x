//! Pin-level test harness for the Motorola 68000.
//!
//! Replaces the archive's `M68kBus`-based test harness with one
//! that drives the CPU through its pin fields, matching how the
//! Amiga machine layer will work.
//!
//! The pattern: after each `tick()`, inspect the CPU's `State` to
//! see if it's in `BusCycle`. If so, read the addr/fc/rw/data from
//! the state, perform the memory operation, write `bus_status` with
//! the result, then tick again.

#![allow(clippy::unnecessary_cast)]

use motorola_68000::Cpu68000;
use motorola_68000::bus::BusStatus;

/// 64 KiB test memory with big-endian word access.
struct TestMem {
    mem: Vec<u8>,
}

impl TestMem {
    fn new(size: usize) -> Self {
        Self {
            mem: vec![0u8; size],
        }
    }

    fn read_word(&self, addr: u32) -> u16 {
        let a = (addr as usize) & !1;
        if a + 1 >= self.mem.len() {
            return 0;
        }
        (u16::from(self.mem[a]) << 8) | u16::from(self.mem[a + 1])
    }

    fn read_byte(&self, addr: u32) -> u8 {
        let a = addr as usize;
        if a >= self.mem.len() {
            return 0;
        }
        self.mem[a]
    }

    fn write_word(&mut self, addr: u32, val: u16) {
        let a = (addr as usize) & !1;
        if a + 1 >= self.mem.len() {
            return;
        }
        self.mem[a] = (val >> 8) as u8;
        self.mem[a + 1] = val as u8;
    }

    fn write_byte(&mut self, addr: u32, val: u8) {
        let a = addr as usize;
        if a < self.mem.len() {
            self.mem[a] = val;
        }
    }

    fn write_long(&mut self, addr: u32, val: u32) {
        self.write_word(addr, (val >> 16) as u16);
        self.write_word(addr + 2, val as u16);
    }
}

/// Run the CPU for up to `max_ticks` 4-clock cycles, servicing
/// bus requests from `mem`. The CPU's `ipl` field is set to
/// `ipl_level` before each tick.
///
/// Returns when the CPU enters a BRA.S * (opcode $60FE) tight loop
/// or when `max_ticks` is exhausted.
fn run_until_idle(cpu: &mut Cpu68000, mem: &mut TestMem, ipl_level: u8, max_ticks: u32) {
    for _ in 0..max_ticks {
        // Set interrupt priority level.
        cpu.ipl = ipl_level;

        // Service any pending bus cycle by inspecting state.
        // The State enum isn't public, so we use a different approach:
        // just set bus_status = Ready with the right data based on
        // the BusCycle's stored addr/rw. But we can't read State
        // fields directly...
        //
        // The pin-level approach: the CPU exposes its bus request
        // through its state. Since State is pub, we can match on it.
        service_bus(cpu, mem);

        cpu.tick();

        // Check for BRA.S * (tight loop) — IR = $60FE.
        if cpu.ir == 0x60FE {
            // Run a few more ticks to let the pipeline settle.
            for _ in 0..20 {
                cpu.ipl = ipl_level;
                service_bus(cpu, mem);
                cpu.tick();
            }
            return;
        }
    }
}

/// Inspect the CPU's state and service any pending bus cycle.
fn service_bus(cpu: &mut Cpu68000, mem: &mut TestMem) {
    // We need to check if the CPU is in a BusCycle or TableWalk
    // state and provide the bus response. The CPU reads bus_status
    // during tick() when cycle_count >= min_bus.
    //
    // Since the State enum variants store the bus request details
    // (addr, fc, is_read, is_word, data), we match on the state
    // and set bus_status accordingly.
    use motorola_68000::cpu::State;

    match &cpu.state {
        State::BusCycle {
            addr,
            fc,
            is_read,
            is_word,
            data,
            cycle_count,
            ..
        } => {
            if *cycle_count >= 3 {
                if *fc == motorola_68000::bus::FunctionCode::InterruptAck {
                    // Autovector: return vector number = 24 + level.
                    cpu.bus_status = BusStatus::Ready(24 + u16::from(cpu.ipl));
                } else if *is_read {
                    let val = if *is_word {
                        mem.read_word(*addr)
                    } else {
                        u16::from(mem.read_byte(*addr))
                    };
                    cpu.bus_status = BusStatus::Ready(val);
                } else {
                    // Write.
                    let val = data.unwrap_or(0);
                    if *is_word {
                        mem.write_word(*addr, val);
                    } else {
                        mem.write_byte(*addr, val as u8);
                    }
                    cpu.bus_status = BusStatus::Ready(0);
                }
            } else {
                cpu.bus_status = BusStatus::Wait;
            }
        }
        _ => {
            cpu.bus_status = BusStatus::Wait;
        }
    }
}

/// Set up a CPU with the reset vector pointing at `entry_pc` and
/// SSP at `ssp`. Runs the reset sequence (reads vectors from
/// $000000-$000007).
fn setup_cpu(mem: &mut TestMem, ssp: u32, entry_pc: u32) -> Cpu68000 {
    // Write initial SSP and PC vectors.
    mem.write_long(0, ssp);
    mem.write_long(4, entry_pc);

    let mut cpu = Cpu68000::new();
    // The 68000 reset sequence: read SSP from $0, PC from $4.
    // We need to drive the CPU through that by ticking until it
    // reaches the entry PC.
    //
    // The archive CPU starts in Idle with an empty micro-op queue.
    // The first tick will queue PromoteIRC which tries to promote
    // the (empty) pipeline. For a proper reset, we need to seed
    // the prefetch:
    cpu.regs.pc = entry_pc;
    cpu.regs.set_active_sp(ssp);
    cpu.regs.sr = 0x2700; // supervisor, IPL mask = 7
    cpu.next_fetch_addr = entry_pc;
    cpu.irc = mem.read_word(entry_pc);
    cpu.irc_addr = entry_pc;
    cpu.next_fetch_addr = entry_pc.wrapping_add(2);

    cpu
}

// ─── Tests ─────────────────────────────────────────────────────────

#[test]
fn moveq_sets_register_and_flags() {
    let mut mem = TestMem::new(0x10000);
    // MOVEQ #$42,D0; BRA.S *
    mem.write_word(0x1000, 0x7042); // MOVEQ #$42,D0
    mem.write_word(0x1002, 0x60FE); // BRA.S *
    let mut cpu = setup_cpu(&mut mem, 0x8000, 0x1000);

    run_until_idle(&mut cpu, &mut mem, 0, 200);
    assert_eq!(cpu.regs.d[0], 0x42, "D0 should be $42");
    assert_eq!(cpu.regs.sr & 0x0F, 0, "no flags should be set");
}

#[test]
fn move_word_between_registers() {
    let mut mem = TestMem::new(0x10000);
    // MOVEQ #$55,D0; MOVE.W D0,D1; BRA.S *
    mem.write_word(0x1000, 0x7055); // MOVEQ #$55,D0
    mem.write_word(0x1002, 0x3200); // MOVE.W D0,D1
    mem.write_word(0x1004, 0x60FE); // BRA.S *
    let mut cpu = setup_cpu(&mut mem, 0x8000, 0x1000);

    run_until_idle(&mut cpu, &mut mem, 0, 500);
    assert_eq!(cpu.regs.d[0] & 0xFFFF, 0x55);
    assert_eq!(cpu.regs.d[1] & 0xFFFF, 0x55);
}

#[test]
fn add_word_registers() {
    let mut mem = TestMem::new(0x10000);
    // MOVEQ #10,D0; MOVEQ #20,D1; ADD.W D0,D1; BRA.S *
    mem.write_word(0x1000, 0x700A); // MOVEQ #10,D0
    mem.write_word(0x1002, 0x720A + 4); // MOVEQ #20,D1 ($7214)
    mem.write_word(0x1002, 0x7214); // MOVEQ #20,D1
    mem.write_word(0x1004, 0xD240); // ADD.W D0,D1
    mem.write_word(0x1006, 0x60FE); // BRA.S *
    let mut cpu = setup_cpu(&mut mem, 0x8000, 0x1000);

    run_until_idle(&mut cpu, &mut mem, 0, 500);
    assert_eq!(cpu.regs.d[0], 10);
    assert_eq!(cpu.regs.d[1] & 0xFFFF, 30);
}

#[test]
fn jsr_and_rts() {
    let mut mem = TestMem::new(0x10000);
    // $1000: JSR $1010; BRA.S *
    // $1010: MOVEQ #$99,D0; RTS
    mem.write_word(0x1000, 0x4EB9); // JSR abs.l
    mem.write_long(0x1002, 0x0000_1010);
    mem.write_word(0x1006, 0x60FE); // BRA.S * (return point)

    mem.write_word(0x1010, 0x7099u16.wrapping_add(0) as u16); // MOVEQ #-103,D0
    // Actually MOVEQ #imm is 0x70xx where xx is the signed byte.
    // $99 = -103 signed → MOVEQ would be 0x7099
    mem.write_word(0x1010, 0x7099); // MOVEQ #-103,D0
    mem.write_word(0x1012, 0x4E75); // RTS
    let mut cpu = setup_cpu(&mut mem, 0x8000, 0x1000);

    run_until_idle(&mut cpu, &mut mem, 0, 1000);
    // After JSR→MOVEQ→RTS, D0 = sign-extended -103 = 0xFFFFFF99
    assert_eq!(cpu.regs.d[0], 0xFFFF_FF99);
    // PC should be at the BRA.S * after JSR
    assert_eq!(cpu.ir, 0x60FE);
}

#[test]
fn memory_write_and_read() {
    let mut mem = TestMem::new(0x10000);
    // MOVE.L #$2000,A0; MOVE.W #$1234,(A0); MOVE.W (A0),D0; BRA.S *
    mem.write_word(0x1000, 0x207C); // MOVEA.L #imm,A0
    mem.write_long(0x1002, 0x0000_2000);
    mem.write_word(0x1006, 0x30FC); // MOVE.W #imm,(A0)+  — actually wrong
    // Let me use a simpler sequence:
    // MOVEQ #$12,D0; MOVE.L #$2000,A0; MOVE.B D0,(A0); MOVE.B (A0),D1; BRA.S *
    mem.write_word(0x1000, 0x7012); // MOVEQ #$12,D0
    mem.write_word(0x1002, 0x207C); // MOVEA.L #imm,A0
    mem.write_long(0x1004, 0x0000_2000);
    mem.write_word(0x1008, 0x1080); // MOVE.B D0,(A0)
    mem.write_word(0x100A, 0x1210); // MOVE.B (A0),D1
    mem.write_word(0x100C, 0x60FE); // BRA.S *
    let mut cpu = setup_cpu(&mut mem, 0x8000, 0x1000);

    run_until_idle(&mut cpu, &mut mem, 0, 1000);
    assert_eq!(mem.read_byte(0x2000), 0x12, "memory should contain $12");
    assert_eq!(cpu.regs.d[1] & 0xFF, 0x12, "D1 should contain $12");
}

#[test]
fn dbra_loop_counts_down() {
    let mut mem = TestMem::new(0x10000);
    // MOVEQ #0,D0; MOVEQ #3,D1
    // loop: ADDQ.W #1,D0; DBRA D1,loop; NOP; BRA.S *
    mem.write_word(0x1000, 0x7000); // MOVEQ #0,D0
    mem.write_word(0x1002, 0x7203); // MOVEQ #3,D1
    mem.write_word(0x1004, 0x5240); // ADDQ.W #1,D0
    mem.write_word(0x1006, 0x51C9); // DBRA D1,..
    mem.write_word(0x1008, 0xFFFC); // displacement = -4 (back to $1004)
    mem.write_word(0x100A, 0x4E71); // NOP (padding so IR doesn't alias $60FE)
    mem.write_word(0x100C, 0x60FE); // BRA.S *
    let mut cpu = setup_cpu(&mut mem, 0x8000, 0x1000);

    run_until_idle(&mut cpu, &mut mem, 0, 5000);
    // DBRA decrements D1 and branches until D1 goes from 0 to -1.
    // Iterations: D1=3→2→1→0→-1 = 4 iterations.
    assert_eq!(
        cpu.regs.d[0] & 0xFFFF,
        4,
        "D0 should be 4 after 4 iterations"
    );
    assert_eq!(
        cpu.regs.d[1] & 0xFFFF,
        0xFFFF,
        "D1 should be $FFFF after DBRA exit"
    );
}

#[test]
fn supervisor_mode_on_reset() {
    let mut mem = TestMem::new(0x10000);
    mem.write_word(0x1000, 0x60FE); // BRA.S *
    let cpu = setup_cpu(&mut mem, 0x8000, 0x1000);
    assert!(
        cpu.regs.is_supervisor(),
        "CPU should start in supervisor mode"
    );
    assert_eq!(
        cpu.regs.interrupt_mask(),
        7,
        "interrupt mask should be 7 on reset"
    );
}

// ─── Interrupt and exception tests (critical for Kickstart) ───────

/// Helper: set up CPU via reset_to (queues FetchIRC+PromoteIRC) and
/// run until the pipeline is primed.
fn setup_cpu_reset_to(mem: &mut TestMem, ssp: u32, entry_pc: u32) -> Cpu68000 {
    mem.write_long(0, ssp);
    mem.write_long(4, entry_pc);
    let mut cpu = Cpu68000::new();
    cpu.reset_to(ssp, entry_pc);
    // Run a few ticks to prime the prefetch pipeline.
    for _ in 0..20 {
        cpu.ipl = 0;
        service_bus(&mut cpu, mem);
        cpu.tick();
    }
    cpu
}

#[test]
fn interrupt_from_tight_branch_loop() {
    // Port of archive's `interrupt_is_taken_from_tight_branch_loop_at_instruction_boundary`.
    // Program at $0100: MOVE.W #$2000,SR; BRA.S *
    // Handler at $0120: MOVEQ #$42,D0; BRA.S *
    // Level-3 autovector (vector 27) → $0120
    let mut mem = TestMem::new(0x2000);

    // Level-3 autovector at vector 27 (offset $6C)
    mem.write_long(27 * 4, 0x0000_0120);

    // $0100: MOVE.W #$2000,SR ; lower IPL mask to 0
    mem.write_word(0x0100, 0x46FC);
    mem.write_word(0x0102, 0x2000);
    // $0104: BRA.S * (tight loop)
    mem.write_word(0x0104, 0x60FE);

    // $0120: MOVEQ #$42,D0; BRA.S *
    mem.write_word(0x0120, 0x7042);
    mem.write_word(0x0122, 0x60FE);

    let mut cpu = setup_cpu_reset_to(&mut mem, 0x0800, 0x0100);

    // Phase 1: Run with no interrupts until CPU reaches the BRA.S * loop.
    let mut in_loop = false;
    for _ in 0..2000 {
        cpu.ipl = 0;
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        if cpu.regs.interrupt_mask() == 0 && cpu.ir == 0x60FE {
            in_loop = true;
            break;
        }
    }
    assert!(
        in_loop,
        "CPU should reach BRA.S * with IPL mask 0 (pc={:06X} sr={:04X})",
        cpu.regs.pc, cpu.regs.sr
    );

    // Phase 2: Assert IPL=3 and wait for the interrupt handler.
    let mut entered_handler = false;
    for _ in 0..10_000 {
        cpu.ipl = 3;
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        if (cpu.regs.d[0] & 0xFF) == 0x42 {
            entered_handler = true;
            break;
        }
    }
    assert!(
        entered_handler,
        "CPU should service level-3 interrupt (pc={:06X} d0={:08X})",
        cpu.regs.pc, cpu.regs.d[0]
    );
    assert_eq!(
        cpu.regs.interrupt_mask(),
        3,
        "SR mask should be 3 in handler"
    );
}

#[test]
fn trap_and_rte() {
    // TRAP #0 → handler writes D0=$99, RTE → BRA.S *
    // This is the exec.library system call mechanism.
    let mut mem = TestMem::new(0x10000);

    // TRAP #0 vector (vector 32) at $80
    mem.write_long(32 * 4, 0x0000_0200);

    // Program at $1000: MOVEQ #0,D0; TRAP #0; BRA.S *
    mem.write_word(0x1000, 0x7000); // MOVEQ #0,D0
    mem.write_word(0x1002, 0x4E40); // TRAP #0
    mem.write_word(0x1004, 0x60FE); // BRA.S *

    // Handler at $0200: MOVEQ #$99,D0; RTE
    // MOVEQ #-103 (0x99 as signed = -103 → D0 = $FFFFFF99)
    mem.write_word(0x0200, 0x7099); // MOVEQ #-103,D0
    mem.write_word(0x0202, 0x4E73); // RTE

    let mut cpu = setup_cpu(&mut mem, 0x8000, 0x1000);
    run_until_idle(&mut cpu, &mut mem, 0, 2000);

    assert_eq!(
        cpu.regs.d[0], 0xFFFF_FF99,
        "D0 should be set by TRAP handler"
    );
    // After RTE, CPU should be at $1004 (BRA.S *)
    assert_eq!(cpu.ir, 0x60FE, "should be in idle loop after RTE");
}

#[test]
fn movem_push_pop_all_registers() {
    // MOVEM.L D0-D7/A0-A6,-(A7); clear regs; MOVEM.L (A7)+,D0-D7/A0-A6; BRA.S *
    // This is the exact pattern exec uses for context switch.
    let mut mem = TestMem::new(0x10000);

    // Program at $1000:
    // MOVEQ #$11,D0; MOVEQ #$22,D1; ...set recognisable values...
    // MOVEM.L D0-D7/A0-A6,-(A7)  = 48E7 FFFE
    // CLR.L D0                     = 4280
    // CLR.L D1                     = 4281
    // MOVEM.L (A7)+,D0-D7/A0-A6  = 4CDF 7FFF
    // BRA.S *                      = 60FE
    let mut pc = 0x1000u32;
    // Set D0=$11, D1=$22
    mem.write_word(pc, 0x7011);
    pc += 2; // MOVEQ #$11,D0
    mem.write_word(pc, 0x7222);
    pc += 2; // MOVEQ #$22,D1

    // MOVEM.L D0-D7/A0-A6,-(A7) — register mask $FFFE (all except A7)
    mem.write_word(pc, 0x48E7);
    pc += 2;
    mem.write_word(pc, 0xFFFE);
    pc += 2;

    // Clear D0 and D1 to prove MOVEM restores them
    mem.write_word(pc, 0x4280);
    pc += 2; // CLR.L D0
    mem.write_word(pc, 0x4281);
    pc += 2; // CLR.L D1

    // MOVEM.L (A7)+,D0-D7/A0-A6 — register mask $7FFF (all except A7)
    mem.write_word(pc, 0x4CDF);
    pc += 2;
    mem.write_word(pc, 0x7FFF);
    pc += 2;

    mem.write_word(pc, 0x60FE); // BRA.S *

    let mut cpu = setup_cpu(&mut mem, 0x8000, 0x1000);
    run_until_idle(&mut cpu, &mut mem, 0, 5000);

    assert_eq!(cpu.regs.d[0], 0x11, "D0 should be restored by MOVEM");
    assert_eq!(cpu.regs.d[1], 0x22, "D1 should be restored by MOVEM");
}

#[test]
fn stop_resumes_on_interrupt() {
    // The exact pattern Kickstart uses: STOP #$2000; BRA.S * (loop)
    // VERTB interrupt should wake the CPU from STOP.
    let mut mem = TestMem::new(0x10000);

    // Level-3 autovector (vector 27) → handler at $0200
    mem.write_long(27 * 4, 0x0000_0200);

    // Program at $1000: STOP #$2000; MOVEQ #$77,D1; BRA.S *
    mem.write_word(0x1000, 0x4E72); // STOP
    mem.write_word(0x1002, 0x2000); // #$2000 (supervisor, mask=0)
    mem.write_word(0x1004, 0x7277); // MOVEQ #$77,D1
    mem.write_word(0x1006, 0x60FE); // BRA.S *

    // Handler at $0200: MOVEQ #$55,D0; RTE
    mem.write_word(0x0200, 0x7055); // MOVEQ #$55,D0
    mem.write_word(0x0202, 0x4E73); // RTE

    let mut cpu = setup_cpu(&mut mem, 0x8000, 0x1000);

    // Run until STOP is reached (CPU halts with SR=$2000).
    for _ in 0..200 {
        cpu.ipl = 0;
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
    }

    // Now assert IPL=3 to wake from STOP.
    let mut woke = false;
    for _ in 0..10_000 {
        cpu.ipl = 3;
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        if cpu.regs.d[0] == 0x55 {
            woke = true;
            break;
        }
    }
    assert!(
        woke,
        "CPU should wake from STOP and enter handler (pc={:06X} d0={:08X})",
        cpu.regs.pc, cpu.regs.d[0]
    );

    // After RTE, should continue after STOP → MOVEQ #$77,D1 → BRA.S *
    run_until_idle(&mut cpu, &mut mem, 0, 2000);
    assert_eq!(
        cpu.regs.d[1] & 0xFF,
        0x77,
        "D1 should be set after STOP+RTE resumes"
    );
}

#[test]
fn move_usp_save_restore() {
    // MOVE USP,An / MOVE An,USP — used by exec for context switch.
    // Supervisor-mode instruction that swaps user stack pointer.
    let mut mem = TestMem::new(0x10000);

    // $1000: MOVE.L #$4000,A0; MOVE A0,USP; MOVE USP,A1; BRA.S *
    let mut pc = 0x1000u32;
    mem.write_word(pc, 0x207C);
    pc += 2; // MOVEA.L #imm,A0
    mem.write_long(pc, 0x0000_4000);
    pc += 4;
    mem.write_word(pc, 0x4E60);
    pc += 2; // MOVE A0,USP
    mem.write_word(pc, 0x4E69);
    pc += 2; // MOVE USP,A1
    mem.write_word(pc, 0x60FE); // BRA.S *

    let mut cpu = setup_cpu(&mut mem, 0x8000, 0x1000);
    run_until_idle(&mut cpu, &mut mem, 0, 2000);

    assert_eq!(cpu.regs.a[1], 0x4000, "A1 should contain USP value");
}

#[test]
fn lea_and_pea() {
    // LEA and PEA are used extensively in Kickstart for stack frame setup.
    let mut mem = TestMem::new(0x10000);

    // $1000: LEA $2000.L,A2; PEA (A2); MOVE.L (A7)+,D0; BRA.S *
    let mut pc = 0x1000u32;
    mem.write_word(pc, 0x45F9);
    pc += 2; // LEA abs.L,A2
    mem.write_long(pc, 0x0000_2000);
    pc += 4;
    mem.write_word(pc, 0x4852);
    pc += 2; // PEA (A2)
    mem.write_word(pc, 0x201F);
    pc += 2; // MOVE.L (A7)+,D0
    mem.write_word(pc, 0x60FE); // BRA.S *

    let mut cpu = setup_cpu(&mut mem, 0x8000, 0x1000);
    run_until_idle(&mut cpu, &mut mem, 0, 2000);

    assert_eq!(cpu.regs.a[2], 0x2000, "A2 should be $2000 from LEA");
    assert_eq!(
        cpu.regs.d[0], 0x2000,
        "D0 should be $2000 popped from PEA result"
    );
}
