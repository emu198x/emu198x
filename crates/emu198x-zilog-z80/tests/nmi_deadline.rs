//! Arrival/pulse sweep pinned against Perfect Z80 9b0d2e5e, including the
//! handler's first PUSH AF. See test-data/z80-nmi-deadline-validation.md.
use emu198x_zilog_z80::Z80;

fn tick(cpu: &mut Z80, memory: &mut [u8; 65536]) {
    cpu.tick();
    if cpu.mreq && cpu.rd {
        cpu.data_in = memory[usize::from(cpu.addr)];
    } else if cpu.mreq && cpu.wr {
        memory[usize::from(cpu.addr)] = cpu.data;
    } else if cpu.iorq && cpu.rd && !cpu.m1 {
        cpu.data_in = (cpu.addr >> 8) as u8;
    }
}

#[test]
fn arrival_deadline_retains_short_pulses_and_snapshot_continuation() {
    // bytes, duration, early/late return PC, early/late pushed AF, next duration
    for (program, duration, early_pc, late_pc, early_af, late_af, next_duration) in [
        (&[0x00][..], 8, 1, 2, 0x34ab, 0x34ab, 8),
        (&[0x3e, 0][..], 14, 2, 3, 0x00ab, 0x00ab, 8),
        (&[0x77][..], 14, 1, 2, 0x34ab, 0x34ab, 8),
        (&[0x09][..], 22, 1, 2, 0x34b0, 0x34b0, 8),
        (&[0x18, 0][..], 24, 2, 3, 0x34ab, 0x34ab, 8),
        (&[0xed, 0xb3][..], 42, 0, 0, 0x3403, 0x3404, 42),
        (&[0x76][..], 8, 1, 1, 0x34ab, 0x34ab, 8),
    ] {
        for arrival in 0..duration + 2 {
            for pulse in [0, 1, 2, 3] {
                let mut cpu = Z80::new();
                cpu.regs.af = 0x34ab;
                cpu.regs.bc = 0x03e0;
                cpu.regs.hl = 0x1d7c;
                cpu.regs.de = 0x41b9;
                cpu.regs.sp = 0x9002;
                cpu.regs.r = 7;
                let mut memory = [0; 65536];
                memory[..program.len()].copy_from_slice(program);
                memory[0x1d7c] = 0x9d;
                memory[0x66] = 0xf5;
                memory[0x67] = 0x76;
                // Align relative phase zero to the first M1 read strobe.
                while !(cpu.m1 && cpu.mreq && cpu.rd) {
                    tick(&mut cpu, &mut memory);
                }
                let late = arrival >= duration - 3;
                let expected_phase = duration + 44 + if late { next_duration } else { 0 };
                let mut restored: Option<(Z80, [u8; 65536])> = None;
                for phase in 0..=expected_phase {
                    if phase != 0 {
                        tick(&mut cpu, &mut memory);
                        if let Some((copy, ram)) = &mut restored {
                            tick(copy, ram);
                            assert_eq!(
                                postcard::to_allocvec(copy).expect("encode"),
                                postcard::to_allocvec(&cpu).expect("encode")
                            );
                            assert_eq!(*ram, memory);
                        }
                    }
                    if phase == arrival {
                        cpu.nmi = true;
                    }
                    if pulse > 0 && phase == arrival + pulse {
                        cpu.nmi = false;
                    }
                    if let Some((copy, _)) = &mut restored {
                        copy.nmi = cpu.nmi;
                    }
                    // Restore both not-yet-eligible states, then continue
                    // through the deferred instruction and interrupt response.
                    if phase == arrival + 1 || phase == arrival + 2 {
                        let bytes = postcard::to_allocvec(&cpu).expect("encode");
                        let mut copy: Z80 = postcard::from_bytes(&bytes).expect("decode");
                        copy.rehydrate_walker_sequence();
                        restored = Some((copy, memory));
                    }
                    let handler = cpu.m1 && cpu.mreq && cpu.rd && cpu.addr == 0x67;
                    assert_eq!(
                        handler,
                        phase == expected_phase,
                        "program={program:02x?} arrival={arrival} pulse={pulse} phase={phase}"
                    );
                }
                assert_eq!(
                    u16::from_le_bytes([memory[0x9000], memory[0x9001]]),
                    if late { late_pc } else { early_pc }
                );
                assert_eq!(
                    u16::from_le_bytes([memory[0x8ffe], memory[0x8fff]]),
                    if late { late_af } else { early_af }
                );
                assert_eq!(cpu.regs.sp, 0x8ffe);
            }
        }
    }
}

#[test]
fn pulse_during_wait_is_retained_and_eligible_nmi_precedes_irq() {
    let mut cpu = Z80::new();
    let mut memory = [0; 65536];
    cpu.regs.sp = 0x9002;
    cpu.regs.im = 1;
    cpu.regs.iff1 = true;
    cpu.regs.iff2 = true;
    cpu.wait = true;
    for _ in 0..12 {
        tick(&mut cpu, &mut memory);
    }
    assert_eq!(cpu.instructions_retired(), 0);
    cpu.nmi = true;
    tick(&mut cpu, &mut memory);
    cpu.nmi = false;
    for _ in 0..5 {
        tick(&mut cpu, &mut memory);
    }
    assert_eq!(cpu.instructions_retired(), 0);
    cpu.irq = true;
    cpu.wait = false;
    for _ in 0..100 {
        tick(&mut cpu, &mut memory);
        if cpu.m1 && cpu.mreq && cpu.rd && cpu.addr == 0x66 {
            assert_eq!(cpu.regs.sp, 0x9000);
            assert_eq!(&memory[0x9000..0x9002], &[1, 0]);
            assert!(!cpu.regs.iff1);
            assert!(cpu.regs.iff2);
            return;
        }
        assert!(
            !(cpu.m1 && cpu.mreq && cpu.rd && cpu.addr == 0x38),
            "IRQ must not win over an eligible NMI"
        );
    }
    panic!("latched pulse was lost during WAIT");
}
