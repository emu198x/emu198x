//! MC68030 external cache-disable integration tests.
//!
//! CDIS is a combinational board input rather than a CACR bit. It must
//! suppress both use and allocation of cache entries without invalidating
//! them. These tests observe external program reads so a cached word cannot
//! be mistaken for an equivalent bus fetch.

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use motorola_68000::bus::{BusStatus, FunctionCode};
use motorola_68000::cpu::State;
use motorola_68030::Cpu68030;

const PC: u32 = 0x0000_1000;

struct Mem {
    bytes: HashMap<u32, u8>,
}

impl Mem {
    fn new() -> Self {
        let mut mem = Self {
            bytes: HashMap::new(),
        };
        mem.write_word(PC, 0x4E71); // NOP
        mem.write_word(PC + 2, 0x51C8); // DBF D0, <disp>
        mem.write_word(PC + 4, 0xFFFC); // target PC
        for index in 0..6 {
            mem.write_word(PC + 6 + index * 2, 0x4E71);
        }
        mem
    }

    fn read_byte(&self, addr: u32) -> u8 {
        *self.bytes.get(&addr).unwrap_or(&0)
    }

    fn read_word(&self, addr: u32) -> u16 {
        (u16::from(self.read_byte(addr)) << 8) | u16::from(self.read_byte(addr.wrapping_add(1)))
    }

    fn write_word(&mut self, addr: u32, value: u16) {
        self.bytes.insert(addr, (value >> 8) as u8);
        self.bytes.insert(addr.wrapping_add(1), value as u8);
    }
}

fn run_loop(cpu: &mut Cpu68030, mem: &Mem, iterations: u16) -> u32 {
    cpu.regs.sr |= 0x2000;
    cpu.regs.d[0] = u32::from(iterations);
    cpu.regs.pc = PC + 4;
    cpu.setup_prefetch(0x4E71, 0x51C8);

    let mut program_reads = 0;
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
                    let value = if *is_word {
                        mem.read_word(*addr)
                    } else {
                        u16::from(mem.read_byte(*addr))
                    };
                    cpu.bus_status = BusStatus::Ready(value);
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

        if (cpu.regs.d[0] & 0xFFFF) == 0xFFFF && matches!(cpu.state, State::Idle) {
            return program_reads;
        }
    }

    panic!("DBF loop did not complete");
}

#[test]
fn cdis_suppresses_hits_without_flushing() {
    let mut cpu = Cpu68030::new();
    let mem = Mem::new();
    cpu.regs.cacr = 0x0000_0001; // EI
    let cache = cpu.variant_icache.as_mut().expect("MC68030 I-cache");
    cache.fill(PC, true, mem.read_word(PC));
    cache.fill(PC + 2, true, mem.read_word(PC + 2));
    cache.fill(PC + 4, true, mem.read_word(PC + 4));

    cpu.set_cdis_asserted(true);
    let disabled_reads = run_loop(&mut cpu, &mem, 16);
    assert_eq!(
        cpu.variant_icache
            .as_ref()
            .expect("MC68030 I-cache")
            .lookup(PC, true),
        Some(mem.read_word(PC)),
        "asserting CDIS must not invalidate a valid entry"
    );

    cpu.set_cdis_asserted(false);
    let enabled_reads = run_loop(&mut cpu, &mem, 16);

    assert!(
        enabled_reads * 3 < disabled_reads,
        "negating CDIS should make retained entries usable again \
         ({enabled_reads} enabled vs {disabled_reads} disabled reads)"
    );
}

#[test]
fn cdis_suppresses_fills() {
    let mut cpu = Cpu68030::new();
    let mem = Mem::new();
    cpu.regs.cacr = 0x0000_0001; // EI

    cpu.set_cdis_asserted(true);
    let _ = run_loop(&mut cpu, &mem, 8);
    assert_eq!(
        cpu.variant_icache
            .as_ref()
            .expect("MC68030 I-cache")
            .lookup(PC, true),
        None
    );

    cpu.set_cdis_asserted(false);
    let _ = run_loop(&mut cpu, &mem, 8);
    assert_eq!(
        cpu.variant_icache
            .as_ref()
            .expect("MC68030 I-cache")
            .lookup(PC, true),
        Some(mem.read_word(PC))
    );
}
