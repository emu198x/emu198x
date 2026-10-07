//! MC68020UM §4.1: instruction prefetch retains both words, including with E=0.
use motorola_68000::bus::{
    BusStatus, DataPortSize, TransferSize, dynamic_transfer_bytes, place_dynamic_read_data,
};
use motorola_68000::cpu::State;
use motorola_68000::microcode::MicroOp;
use motorola_68020::Cpu68020;

fn cpu(cache: bool) -> Cpu68020 {
    let mut cpu = Cpu68020::new();
    cpu.regs.sr = 0x2000;
    cpu.regs.cacr = u32::from(cache);
    cpu.regs.pc = 0x1004;
    cpu.setup_prefetch(0x4E71, 0x4E71);
    cpu
}

fn word(address: u32, sibling: u16) -> u16 {
    match address {
        0x1006 => sibling,
        0x1008 => 0x60FE,
        _ => 0x4E71,
    }
}

fn clock(
    cpu: &mut Cpu68020,
    sibling: u16,
    port: Option<DataPortSize>,
) -> Option<(u32, TransferSize)> {
    cpu.bus_status = BusStatus::Wait;
    let mut phase = None;
    if let State::BusCycle {
        addr,
        op: MicroOp::FetchIRC,
        cycle_count,
        ..
    } = cpu.state
        && cycle_count >= cpu.variant_min_bus_clocks
    {
        phase = Some((addr, cpu.bus_transfer_size));
        cpu.bus_status = if let Some(port) = port {
            let count = dynamic_transfer_bytes(cpu.bus_transfer_size, addr, port);
            let mut packed = 0;
            for offset in 0..count {
                let address = addr + u32::from(offset);
                let value = word(address & !1, sibling);
                let byte = if address & 1 == 0 {
                    value >> 8
                } else {
                    value & 255
                };
                packed = (packed << 8) | u32::from(byte);
            }
            BusStatus::ReadySized {
                data: place_dynamic_read_data(packed, count, addr, port),
                port,
            }
        } else {
            BusStatus::Ready(word(addr, sibling))
        };
    }
    cpu.tick();
    phase
}

#[test]
fn sibling_instruction_is_retained_with_cache_on_or_off_on_every_port() {
    for enabled in [false, true] {
        for port in [
            None,
            Some(DataPortSize::Byte),
            Some(DataPortSize::Word),
            Some(DataPortSize::Long),
        ] {
            let mut cpu = cpu(enabled);
            let mut changed = false;
            let mut phases = Vec::new();
            for _ in 0..200 {
                if let Some(phase) = clock(&mut cpu, if changed { 0x7E02 } else { 0x7E01 }, port) {
                    phases.push(phase);
                }
                if cpu.next_fetch_addr == 0x1006 && matches!(cpu.state, State::Idle) {
                    changed = true;
                }
            }
            assert!(changed, "prefetch completion not reached");
            assert_eq!(cpu.regs.d[7], 1, "port={port:?} cache={enabled}");
            let first: Vec<_> = phases
                .into_iter()
                .filter(|(addr, _)| *addr < 0x1008)
                .collect();
            let expected = match port {
                Some(DataPortSize::Byte) => vec![
                    (0x1004, TransferSize::Long),
                    (0x1005, TransferSize::ThreeBytes),
                    (0x1006, TransferSize::Word),
                    (0x1007, TransferSize::Byte),
                ],
                Some(DataPortSize::Long) => vec![(0x1004, TransferSize::Long)],
                _ => vec![(0x1004, TransferSize::Long), (0x1006, TransferSize::Word)],
            };
            assert_eq!(first, expected, "port={port:?} cache={enabled}");
        }
    }
}

#[test]
fn partial_prefetch_and_holding_register_restore_tick_for_tick()
-> Result<(), Box<dyn std::error::Error>> {
    for completed in 1..=4 {
        let mut original = cpu(false);
        let mut phases = 0;
        for _ in 0..100 {
            phases += usize::from(clock(&mut original, 0x7E01, Some(DataPortSize::Byte)).is_some());
            if phases == completed {
                break;
            }
        }
        assert_eq!(phases, completed);
        let bytes = postcard::to_allocvec(&original)?;
        let mut restored: Cpu68020 = postcard::from_bytes(&bytes)?;
        assert!(restored.variant_longword_prefetch);
        assert_eq!(bytes, postcard::to_allocvec(&restored)?);
        // Mutation during a partial transfer affects still-unread bytes;
        // mutation after phase four must retain the earlier sibling.
        for _ in 0..150 {
            assert_eq!(
                clock(&mut original, 0x7E02, Some(DataPortSize::Byte)),
                clock(&mut restored, 0x7E02, Some(DataPortSize::Byte))
            );
            assert_eq!(
                postcard::to_allocvec(&original)?,
                postcard::to_allocvec(&restored)?
            );
        }
        assert_eq!(original.regs.d[7], if completed == 4 { 1 } else { 2 });
    }
    Ok(())
}

fn fetch(cpu: &mut Cpu68020, address: u32) -> Vec<(u32, TransferSize)> {
    cpu.next_fetch_addr = address;
    cpu.micro_ops.clear();
    cpu.micro_ops.push(MicroOp::FetchIRC);
    cpu.state = State::Idle;
    let mut phases = Vec::new();
    for _ in 0..100 {
        if let Some(phase) = clock(cpu, 0x7E01, Some(DataPortSize::Long)) {
            phases.push(phase);
        }
        if cpu.next_fetch_addr == address + 2 {
            return phases;
        }
    }
    panic!("prefetch never completed at {address:#x}");
}

#[test]
fn low_word_entry_fills_whole_line_and_holding_remains_independent_of_cache() {
    let mut cpu = cpu(true);
    assert_eq!(fetch(&mut cpu, 0x1006), [(0x1004, TransferSize::Long)]);
    assert_eq!(cpu.irc, 0x7E01);
    let cache = cpu.variant_icache.as_mut().expect("MC68020 cache");
    assert_eq!(cache.lookup(0x1004, true), Some(0x4E71));
    assert_eq!(cache.lookup(0x1006, true), Some(0x7E01));
    cache.fill_long(0x1004, true, 0x7002_7E02);
    assert!(fetch(&mut cpu, 0x1004).is_empty());
    assert_eq!(
        cpu.irc, 0x4E71,
        "holding register retains the preceding fill"
    );
    cpu.variant_icache.as_mut().expect("cache").clear_holding();
    assert!(fetch(&mut cpu, 0x1006).is_empty());
    assert_eq!(
        cpu.irc, 0x7E02,
        "a complete warm line reloads the holding register"
    );
}

#[test]
fn frozen_cache_still_retains_prefetch_and_program_spaces_do_not_alias() {
    let mut cpu = cpu(true);
    cpu.regs.cacr = 3;
    assert_eq!(fetch(&mut cpu, 0x1004).len(), 1);
    assert_eq!(
        cpu.variant_icache
            .as_ref()
            .expect("cache")
            .valid_word_count(),
        0
    );
    assert!(
        fetch(&mut cpu, 0x1006).is_empty(),
        "freeze does not disable holding"
    );
    cpu.regs.sr = 0;
    assert_eq!(
        fetch(&mut cpu, 0x1006),
        [(0x1004, TransferSize::Long)],
        "user fetch must not reuse supervisor holding"
    );
    cpu.reset_to(0x8000, 0x1004);
    assert_eq!(
        cpu.variant_icache
            .as_ref()
            .expect("cache")
            .holding_word(0x1004, false),
        None
    );
}
