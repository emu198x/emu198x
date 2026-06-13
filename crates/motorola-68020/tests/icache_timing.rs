//! 68020 instruction-cache timing tests.
//!
//! The Tom Harte sweep validates architectural *state* but not cycle
//! counts or bus traffic, so it cannot see the I-cache (the served word
//! is identical whether it came from the cache or the bus). These tests
//! exercise the load-bearing property instead: a program-space prefetch
//! that hits the enabled cache asserts **no external bus cycle**, so a
//! re-executed loop stops fetching its body from memory — which on the
//! Amiga is what keeps cached code from contending with Agnus for chip
//! RAM. (M68020UM § 6.)
//!
//! Method: run a tight `DBF` loop and count the program-space read bus
//! cycles. With the cache disabled (CACR.E = 0, the reset state) every
//! iteration re-fetches the body. With it enabled, only the first
//! iteration touches the bus; the rest hit.

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use motorola_68000::bus::{BusStatus, FunctionCode};
use motorola_68000::cpu::State;
use motorola_68020::Cpu68020;

const PC: u32 = 0x0000_1000;

struct Mem {
    bytes: HashMap<u32, u8>,
}

impl Mem {
    fn new() -> Self {
        Self {
            bytes: HashMap::new(),
        }
    }
    fn read_byte(&self, a: u32) -> u8 {
        *self.bytes.get(&(a & 0x00FF_FFFF)).unwrap_or(&0)
    }
    fn read_word(&self, a: u32) -> u16 {
        (u16::from(self.read_byte(a)) << 8) | u16::from(self.read_byte(a.wrapping_add(1)))
    }
    fn write_word(&mut self, a: u32, v: u16) {
        self.bytes.insert(a & 0x00FF_FFFF, (v >> 8) as u8);
        self.bytes
            .insert((a.wrapping_add(1)) & 0x00FF_FFFF, v as u8);
    }
}

/// A `DBF D0, top` loop whose body is a single NOP:
///
/// ```text
///   PC+0:  NOP            ; loop top
///   PC+2:  DBF  D0, PC+0
///   PC+4:  $FFFC          ; displacement = -4 → back to PC+0
/// ```
///
/// With `D0 = n`, the body runs `n + 1` times (n, n-1, …, 0, then -1
/// exits). Trailing NOPs let the post-loop prefetch run off cleanly.
fn write_loop(mem: &mut Mem) {
    mem.write_word(PC, 0x4E71); // NOP
    mem.write_word(PC + 2, 0x51C8); // DBF D0, <disp>
    mem.write_word(PC + 4, 0xFFFC); // disp -4 (target = PC+4-4 = PC)
    for k in 0..6 {
        mem.write_word(PC + 6 + (k as u32) * 2, 0x4E71);
    }
}

/// Run the loop to completion (or a tick ceiling) and return the count
/// of completed **program-space read** bus cycles. `cache_enabled` sets
/// CACR.E before the run.
fn count_program_fetches(iterations: u16, cache_enabled: bool) -> u32 {
    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();
    write_loop(&mut mem);

    cpu.regs.sr |= 0x2000; // supervisor
    cpu.regs.d[0] = u32::from(iterations);
    if cache_enabled {
        cpu.regs.cacr = 0x01; // E: enable instruction cache
    }
    cpu.regs.pc = PC + 4;
    cpu.setup_prefetch(0x4E71, 0x51C8);

    let mut program_reads = 0u32;
    // Generous ceiling: even cache-disabled, the loop is well under this.
    for _ in 0..4000 {
        if let State::BusCycle {
            addr,
            fc,
            is_read,
            is_word,
            cycle_count,
            ..
        } = &cpu.state
        {
            if *cycle_count >= 3 {
                if *is_read {
                    if matches!(
                        fc,
                        FunctionCode::SupervisorProgram | FunctionCode::UserProgram
                    ) {
                        program_reads += 1;
                    }
                    let v = if *is_word {
                        mem.read_word(*addr)
                    } else {
                        u16::from(mem.read_byte(*addr))
                    };
                    cpu.bus_status = BusStatus::Ready(v);
                } else {
                    cpu.bus_status = BusStatus::Ready(0);
                }
            } else {
                cpu.bus_status = BusStatus::Wait;
            }
        } else {
            cpu.bus_status = BusStatus::Wait;
        }

        cpu.tick();

        // DBF decrements only the low word of D0; the loop exits when
        // that word wraps from 0x0000 to 0xFFFF.
        if (cpu.regs.d[0] & 0xFFFF) == 0xFFFF && matches!(cpu.state, State::Idle) {
            break;
        }
    }
    program_reads
}

#[test]
fn enabled_cache_elides_loop_body_refetches() {
    // 16 iterations: enough that re-fetch savings dominate the one-time
    // cold fill.
    let cold = count_program_fetches(16, false);
    let warm = count_program_fetches(16, true);

    // Disabled: every iteration re-fetches the loop body from memory.
    // Enabled: the body is fetched once, then served from the cache.
    assert!(
        warm * 3 < cold,
        "cache-enabled program fetches ({warm}) should be far below \
         cache-disabled ({cold})"
    );
    // Sanity: the cache cannot make fetches *increase*.
    assert!(warm <= cold);
}

#[test]
fn disabled_cache_refetches_every_iteration() {
    // With more iterations the disabled count must grow (no caching);
    // the enabled count stays roughly flat (cold fill + tiny tail).
    let cold_8 = count_program_fetches(8, false);
    let cold_32 = count_program_fetches(32, false);
    assert!(
        cold_32 > cold_8,
        "more iterations must mean more fetches when the cache is off \
         ({cold_8} vs {cold_32})"
    );

    let warm_8 = count_program_fetches(8, true);
    let warm_32 = count_program_fetches(32, true);
    // Enabled: the body lives in 1–2 cache lines, so the fetch count is
    // essentially independent of iteration count.
    assert!(
        warm_32 <= warm_8 + 4,
        "enabled-cache fetch count should be near-constant across \
         iteration counts ({warm_8} vs {warm_32})"
    );
}
