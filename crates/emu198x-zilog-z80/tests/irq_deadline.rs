//! IM1 arrival/pulse outcomes pinned against Perfect Z80 9b0d2e5e.
//! See test-data/z80-irq-deadline-validation.md.
use emu198x_zilog_z80::Z80;

fn tick(cpu: &mut Z80, memory: &mut [u8; 65536]) {
    cpu.tick();
    if cpu.mreq && cpu.rd {
        cpu.data_in = memory[usize::from(cpu.addr)];
    } else if cpu.mreq && cpu.wr {
        memory[usize::from(cpu.addr)] = cpu.data;
    }
}

#[test]
fn irq_samples_level_before_boundary_and_preserves_history_across_snapshots() {
    // Program, first eligible boundary, phase count, return PC, pushed AF.
    // A zero first boundary means DI prevents every request in the sweep.
    for (program, boundary, phases, return_pc, af) in [
        (&[0][..], 8, 10, 1, 0x34ab),
        (&[0x3e, 0][..], 14, 16, 2, 0x00ab),
        (&[0x77][..], 14, 16, 1, 0x34ab),
        (&[0x09][..], 22, 24, 1, 0x34b0),
        (&[0x18, 0][..], 24, 26, 2, 0x34ab),
        (&[0x76][..], 8, 10, 1, 0x34ab),
        (&[0xed, 0xb3][..], 42, 44, 0, 0x3403),
        (&[0xfb, 0][..], 16, 20, 2, 0x34ab),
        (&[0xfb, 0x76][..], 16, 20, 2, 0x34ab),
        (&[0xfb, 0xf3][..], 0, 20, 0, 0x34ab),
        (&[0xfb, 0xfb, 0][..], 24, 28, 3, 0x34ab),
        (&[0xf3][..], 0, 10, 0, 0x34ab),
    ] {
        let repeat = program == [0xed, 0xb3];
        let halted = program.contains(&0x76);
        for arrival in 0..phases {
            for pulse in [0, 1, 2, 3] {
                // The level must cover the final T-state rising edge.
                // Pulses entirely between sampling instants are not latched.
                let expected = (0..4).find_map(|iteration| {
                    if boundary == 0 {
                        return None;
                    }
                    let response_boundary = boundary + iteration * if repeat { 42 } else { 8 };
                    let sample = response_boundary - 3;
                    (arrival < sample && (pulse == 0 || arrival + pulse >= sample))
                        .then_some((response_boundary + 48, iteration))
                });
                let mut cpu = Z80::new();
                cpu.regs.af = 0x34ab;
                cpu.regs.bc = 0x03e0;
                cpu.regs.hl = 0x1d7c;
                cpu.regs.de = 0x41b9;
                cpu.regs.sp = 0x9002;
                cpu.regs.r = 10;
                cpu.regs.im = 1;
                cpu.regs.iff1 = true;
                cpu.regs.iff2 = true;
                let mut memory = [0; 65536];
                memory[..program.len()].copy_from_slice(program);
                memory[0x1d7c] = 0x9d;
                memory[0x38] = 0xf5;
                memory[0x39] = 0x76;
                while !(cpu.m1 && cpu.mreq && cpu.rd) {
                    tick(&mut cpu, &mut memory);
                }
                let mut restored: Option<(Z80, [u8; 65536])> = None;
                let end = expected.map_or(298, |(phase, _)| phase);
                for phase in 0..=end {
                    if phase > 0 {
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
                        cpu.irq = true;
                    }
                    if pulse > 0 && phase == arrival + pulse {
                        cpu.irq = false;
                    }
                    if let Some((copy, _)) = &mut restored {
                        copy.irq = cpu.irq;
                    }
                    if phase == arrival + 1 || phase == arrival + 2 {
                        let bytes = postcard::to_allocvec(&cpu).expect("encode");
                        let mut copy: Z80 = postcard::from_bytes(&bytes).expect("decode");
                        copy.rehydrate_walker_sequence();
                        restored = Some((copy, memory));
                    }
                    let handler = cpu.m1 && cpu.mreq && cpu.rd && cpu.addr == 0x39;
                    assert_eq!(
                        handler,
                        expected.is_some_and(|(when, _)| phase == when),
                        "program={program:02x?} arrival={arrival} pulse={pulse} phase={phase}"
                    );
                }
                if let Some((_, iteration)) = expected {
                    let expected_pc = return_pc
                        + if repeat || halted {
                            0
                        } else {
                            iteration as u16
                        };
                    let expected_af = if repeat && iteration == 1 { 0x3404 } else { af };
                    assert_eq!(
                        u16::from_le_bytes([memory[0x9000], memory[0x9001]]),
                        expected_pc
                    );
                    assert_eq!(
                        u16::from_le_bytes([memory[0x8ffe], memory[0x8fff]]),
                        expected_af
                    );
                    assert_eq!(cpu.regs.sp, 0x8ffe);
                    assert!(!cpu.regs.iff1 && !cpu.regs.iff2);
                } else {
                    assert_eq!(cpu.regs.sp, 0x9002, "no interrupt stack writes expected");
                }
            }
        }
    }
}
