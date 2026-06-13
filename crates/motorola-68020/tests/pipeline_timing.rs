//! 68020 pipeline dead-time tests (#41 Phase 4).
//!
//! The sequential 68000 model spends 2 internal clocks calculating an
//! indexed or predecrement effective address *after* fetching the
//! extension word. The 68020's three-stage pipeline overlaps that
//! calculation with the next fetch/decode, so it costs no observable
//! clocks — the `variant_pipeline_no_ext_delay` flag (set by the 020
//! wrapper) drops the delay.
//!
//! These tests run one instruction to completion and count CPU ticks
//! with the flag on (020 behaviour) vs off (68000 behaviour), holding
//! everything else equal. The address the instruction computes must be
//! identical either way — only the clock count moves.

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use motorola_68000::bus::BusStatus;
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

fn service_bus(cpu: &mut Cpu68020, mem: &mut Mem) {
    if let State::BusCycle {
        addr,
        is_read,
        is_word,
        data,
        cycle_count,
        ..
    } = &cpu.state
    {
        if *cycle_count >= 3 {
            if *is_read {
                let v = if *is_word {
                    mem.read_word(*addr)
                } else {
                    u16::from(mem.read_byte(*addr))
                };
                cpu.bus_status = BusStatus::Ready(v);
            } else {
                let v = data.unwrap_or(0);
                if *is_word {
                    mem.write_word(*addr, v);
                }
                cpu.bus_status = BusStatus::Ready(0);
            }
        } else {
            cpu.bus_status = BusStatus::Wait;
        }
    } else {
        cpu.bus_status = BusStatus::Wait;
    }
}

/// Run `MOVE.W (0,A0,D1.W),D2` once and return `(ticks, d2)`. The source
/// EA is the brief indexed mode `(d8,An,Xn)` — exactly the mode whose
/// post-fetch calculation the 020 pipelines. `pipeline` selects the
/// 020 (true) or 68000 (false) timing for the EA calculation.
fn run_indexed_move(pipeline: bool) -> (u32, u32) {
    const OPCODE: u16 = 0x3430; // MOVE.W (d8,A0,Xn),D2
    const EXT: u16 = 0x1000; // Xn = D1.W, scale 1, disp 0, brief

    let mut cpu = Cpu68020::new();
    let mut mem = Mem::new();

    mem.write_word(PC, OPCODE);
    mem.write_word(PC + 2, EXT);
    for k in 0..4 {
        mem.write_word(PC + 4 + (k as u32) * 2, 0x4E71); // trailing NOPs
    }
    mem.write_word(0x2010, 0xCAFE); // source operand at A0 + D1 = 0x2010

    cpu.regs.sr |= 0x2000; // supervisor
    cpu.regs.a[0] = 0x2000;
    cpu.regs.d[1] = 0x10;
    // Override the flag the wrapper set, to measure both timings on one
    // otherwise-identical 020 (3-clock bus, same prefetch).
    cpu.variant_pipeline_no_ext_delay = pipeline;
    cpu.regs.pc = PC + 4;
    cpu.setup_prefetch(OPCODE, EXT);

    let start = cpu.instruction_starts;
    let mut ticks = 0u32;
    for _ in 0..400 {
        service_bus(&mut cpu, &mut mem);
        cpu.tick();
        ticks += 1;
        if cpu.instruction_starts > start {
            return (ticks, cpu.regs.d[2] & 0xFFFF);
        }
    }
    panic!("instruction did not complete");
}

#[test]
fn pipeline_drops_indexed_ea_dead_time() {
    let (ticks_020, d2_020) = run_indexed_move(true);
    let (ticks_000, d2_000) = run_indexed_move(false);

    // The address computed — and thus the value loaded — is identical;
    // only the clock count differs.
    assert_eq!(d2_020, 0xCAFE);
    assert_eq!(d2_000, 0xCAFE);

    // The 68000 path spends 2 extra internal clocks on the EA
    // calculation; the 020 pipeline hides them.
    assert_eq!(
        ticks_000 - ticks_020,
        2,
        "expected the 020 to save exactly the 2-clock indexed-EA dead \
         time (020 {ticks_020} vs 68000 {ticks_000})"
    );
}
