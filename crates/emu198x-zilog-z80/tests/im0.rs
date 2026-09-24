use emu198x_zilog_z80::{ExecutionFlow, ExecutionKind, Z80};

#[derive(Clone)]
struct Device {
    memory: Box<[u8; 65536]>,
    stream: Vec<u8>,
    position: usize,
    transfer: bool,
    driven: u8,
    reads: Vec<(bool, u16)>,
}
impl Device {
    fn new(stream: &[u8]) -> Self {
        Self {
            memory: Box::new([0; 65536]),
            stream: stream.to_vec(),
            position: 0,
            transfer: false,
            driven: 0,
            reads: Vec::new(),
        }
    }
    fn tick(&mut self, cpu: &mut Z80) {
        cpu.tick();
        let ack = cpu.iorq && cpu.m1;
        let operand = cpu.mreq && cpu.rd && !cpu.m1 && cpu.addr == 0x1000;
        let transfer = ack || operand;
        if transfer {
            if !self.transfer {
                self.driven = *self
                    .stream
                    .get(self.position)
                    .expect("device stream exhausted");
                self.position += 1;
                self.reads.push((ack, cpu.addr));
            }
            cpu.data_in = self.driven;
        } else if cpu.mreq && cpu.rd {
            cpu.data_in = self.memory[usize::from(cpu.addr)];
        } else if cpu.mreq && cpu.wr {
            self.memory[usize::from(cpu.addr)] = cpu.data;
        }
        self.transfer = transfer;
    }
}
fn ready() -> Z80 {
    let mut cpu = Z80::new();
    let mut device = Device::new(&[]);
    for _ in 0..8 {
        device.tick(&mut cpu);
    } // retire NOP, arm boundary sample
    cpu.regs.pc = 0x1000;
    cpu.regs.sp = 0x9000;
    cpu.regs.hl = 0x8021;
    cpu.regs.ix = 0x8031;
    cpu.regs.bc = 0x1234;
    cpu.regs.wz = 0x28ab;
    cpu.regs.im = 0;
    cpu.regs.iff1 = true;
    cpu.regs.iff2 = true;
    cpu.irq = true;
    cpu
}
fn finish(cpu: &mut Z80, device: &mut Device) -> u32 {
    let retired = cpu.instructions_retired();
    for hc in 1..=1000 {
        device.tick(cpu);
        if cpu.instructions_retired() != retired {
            return hc;
        }
    }
    panic!("IM 0 instruction did not retire");
}

#[test]
fn injected_single_byte_operations_use_normal_execution() {
    for (opcode, hc, pc, sp) in [
        (0x00, 12, 0x1000, 0x9000),
        (0xe9, 12, 0x8021, 0x9000),
        (0xcf, 26, 0x0008, 0x8ffe),
        (0xff, 26, 0x0038, 0x8ffe),
        (0xc9, 24, 0x1234, 0x9002),
        (0xc5, 26, 0x1000, 0x8ffe),
        (0x03, 16, 0x1000, 0x9000),
    ] {
        let mut cpu = ready();
        let mut device = Device::new(&[opcode]);
        device.memory[0x9000] = 0x34;
        device.memory[0x9001] = 0x12;
        let r = cpu.regs.r;
        assert_eq!(finish(&mut cpu, &mut device), hc, "opcode {opcode:02x}");
        assert_eq!((cpu.regs.pc, cpu.regs.sp), (pc, sp), "opcode {opcode:02x}");
        assert_eq!(cpu.regs.r, r + 1);
        assert!(!cpu.regs.iff1 && !cpu.regs.iff2);
        assert_eq!(device.position, 1);
        if opcode == 0xe9 || opcode == 0x00 {
            assert_eq!(cpu.regs.wz, 0x28ab);
        }
        if opcode == 0xcf || opcode == 0xff {
            assert_eq!(&device.memory[0x8ffe..0x9000], &[0x00, 0x10]);
        }
    }
}

#[test]
fn injected_operands_and_prefixes_do_not_advance_pc() {
    for (bytes, hc, pc, sp, acks) in [
        (&[0x3e, 0xa5][..], 18, 0x1000, 0x9000, 1),
        (&[0xcd, 0x34, 0x12][..], 38, 0x1234, 0x8ffe, 1),
        (&[0xdd, 0xe9][..], 24, 0x8031, 0x9000, 2),
        (&[0xdd, 0xfd, 0x21, 0x34, 0x12][..], 48, 0x1000, 0x9000, 3),
        (&[0xcb, 0x00][..], 24, 0x1000, 0x9000, 2),
        (&[0xed, 0x44][..], 24, 0x1000, 0x9000, 2),
        (&[0xdd, 0xcb, 0x01, 0x46][..], 48, 0x1000, 0x9000, 2),
    ] {
        let mut cpu = ready();
        let mut device = Device::new(bytes);
        let r = cpu.regs.r;
        assert_eq!(finish(&mut cpu, &mut device), hc, "{bytes:02x?}");
        assert_eq!((cpu.regs.pc, cpu.regs.sp), (pc, sp), "{bytes:02x?}");
        assert_eq!(device.position, bytes.len());
        assert_eq!(device.reads.iter().filter(|(ack, _)| *ack).count(), acks);
        assert!(device.reads.iter().all(|(_, addr)| *addr == 0x1000));
        assert_eq!(cpu.regs.r, r + acks as u8);
        if bytes[0] == 0xcd {
            assert_eq!(&device.memory[0x8ffe..0x9000], &[0, 0x10]);
        }
        if bytes[0] == 0x3e {
            assert_eq!(cpu.regs.a(), 0xa5);
        }
        if bytes.len() == 5 {
            assert_eq!(cpu.regs.iy, 0x1234);
        }
    }
}

#[test]
fn injected_instruction_snapshots_resume_at_every_half_cycle() {
    for bytes in [
        &[0xe9][..],
        &[0xcd, 0x34, 0x12][..],
        &[0xdd, 0xcb, 1, 0x46][..],
    ] {
        let mut control = ready();
        let mut bus = Device::new(bytes);
        let total = finish(&mut control, &mut bus);
        for offset in 0..total {
            let mut cpu = ready();
            let mut device = Device::new(bytes);
            for _ in 0..offset {
                device.tick(&mut cpu);
            }
            let encoded = postcard::to_allocvec(&cpu).expect("encode");
            let mut restored: Z80 = postcard::from_bytes(&encoded).expect("decode");
            restored.rehydrate_walker_sequence();
            for _ in offset..total {
                device.tick(&mut restored);
            }
            assert_eq!(
                postcard::to_allocvec(&restored).expect("encode"),
                postcard::to_allocvec(&control).expect("encode"),
                "{bytes:02x?} offset {offset}"
            );
            assert_eq!(device.memory, bus.memory);
        }
    }
}

#[test]
fn injected_observer_records_only_real_stack_transfers() {
    for bytes in [
        &[0xe9][..],
        &[0x00][..],
        &[0xcf][..],
        &[0xcd, 0x34, 0x12][..],
        &[0xc9][..],
    ] {
        let mut cpu = ready();
        let mut device = Device::new(bytes);
        cpu.start_execution_observation();
        finish(&mut cpu, &mut device);
        let event = cpu.completed_execution_event().expect("retired interrupt");
        assert_eq!(event.kind, ExecutionKind::Interrupt);
        assert_eq!(
            event.flow,
            match bytes[0] {
                0xcf | 0xcd => Some(ExecutionFlow::Interrupt {
                    return_address: 0x1000
                }),
                0xc9 => Some(ExecutionFlow::Return),
                _ => None,
            }
        );
    }
}

#[test]
fn injected_ack_wait_preserves_opcode_and_extends_by_whole_tstates() {
    use emu198x_zilog_z80::z80::{IntAckPhase, Phase};
    let mut cpu = ready();
    let mut device = Device::new(&[0xe9]);
    for _ in 0..8 {
        device.tick(&mut cpu);
    }
    assert_eq!(cpu.phase, Phase::IntAck(IntAckPhase::T5Rise));
    // The byte was driven at IORQ assertion and is sampled on T5 rise.
    cpu.data_in = 0xe9;
    for _ in 0..4 {
        device.tick(&mut cpu);
    }
    assert_eq!(cpu.regs.pc, 0x8021);
    assert_eq!(cpu.regs.sp, 0x9000);

    let mut cpu = ready();
    let mut device = Device::new(&[0xe9]);
    for _ in 0..7 {
        device.tick(&mut cpu);
    }
    assert_eq!(cpu.phase, Phase::IntAck(IntAckPhase::T4Fall));
    cpu.wait = true;
    for _ in 0..6 {
        device.tick(&mut cpu);
    }
    assert_eq!(cpu.phase, Phase::IntAck(IntAckPhase::T4Fall));
    cpu.wait = false;
    assert_eq!(finish(&mut cpu, &mut device), 5);
    assert_eq!(cpu.regs.pc, 0x8021);
    assert_eq!(device.position, 1);
}

#[test]
fn otir_im0_jump_preserves_wz_for_bit_observation() {
    // Banks' Perfect Z80 z80otir.c program. Raise INT during the output,
    // then inject JP(HL), preserving WZ for BIT 0,(HL) and PUSH AF.
    let mut cpu = Z80::new();
    cpu.regs.sp = 0x9000;
    let mut device = Device::new(&[0xe9]);
    device.memory[..17].copy_from_slice(&[
        0, 0xc3, 4, 0, 0xed, 0x46, 0xfb, 0x21, 0x20, 0, 0x01, 1, 0xff, 0xed, 0xb3, 0, 0x76,
    ]);
    device.memory[0x20..0x28].copy_from_slice(&[0, 0xcb, 0x46, 0, 0xf5, 0, 0, 0x76]);
    for _ in 0..400 {
        device.tick(&mut cpu);
        if cpu.iorq && cpu.wr {
            cpu.irq = true;
        }
        if cpu.halt {
            break;
        }
    }
    assert!(cpu.halt);
    assert_eq!(
        (cpu.regs.pc, cpu.regs.wz, cpu.regs.bc),
        (0x28, 0x0e, 0xfe01)
    );
    assert_eq!(cpu.regs.f(), 0x10);
    assert_eq!(cpu.regs.sp, 0x8ffe); // PUSH AF only; JP(HL) does not push
    assert_eq!(device.memory[0x8ffe], 0x10);
    assert_eq!(device.position, 1);
}

#[test]
fn injected_ei_defers_the_next_interrupt_until_one_memory_instruction() {
    let mut cpu = ready();
    let mut device = Device::new(&[0xfb, 0x00]);
    assert_eq!(finish(&mut cpu, &mut device), 12);
    assert!(cpu.regs.iff1 && cpu.regs.iff2);
    assert_eq!(finish(&mut cpu, &mut device), 8); // memory NOP after EI
    assert_eq!((cpu.regs.pc, device.position), (0x1001, 1));
    assert_eq!(finish(&mut cpu, &mut device), 12); // next interrupt injects NOP
    assert_eq!((cpu.regs.pc, device.position), (0x1001, 2));
    assert!(!cpu.regs.iff1 && !cpu.regs.iff2);
}

#[test]
fn nmi_waits_until_an_injected_prefixed_instruction_finishes() {
    let mut cpu = ready();
    let mut device = Device::new(&[0xdd, 0x21, 0x34, 0x12]);
    for _ in 0..12 {
        device.tick(&mut cpu);
    }
    cpu.nmi = true;
    assert_eq!(finish(&mut cpu, &mut device), 24);
    assert_eq!(cpu.regs.ix, 0x1234);
    assert_eq!(cpu.regs.pc, 0x1000);
    assert_eq!(finish(&mut cpu, &mut device), 22);
    assert_eq!(cpu.regs.pc, 0x66);
    assert_eq!(&device.memory[0x8ffe..0x9000], &[0, 0x10]);
}

#[test]
fn injected_scf_uses_the_prior_q_once() {
    let mut cpu = ready();
    let mut device = Device::new(&[0x37]);
    cpu.regs.af = 0x0028;
    cpu.regs.q = 0x28;
    finish(&mut cpu, &mut device);
    assert_eq!(cpu.regs.f(), 0x01);
}

#[test]
fn original_prefix_snapshot_tags_are_unchanged() {
    use emu198x_zilog_z80::walker::Prefix;
    for (tag, prefix) in [
        Prefix::None,
        Prefix::CB,
        Prefix::ED,
        Prefix::DD,
        Prefix::FD,
        Prefix::DDCB,
        Prefix::FDCB,
        Prefix::InterruptNmi,
        Prefix::InterruptIm0,
        Prefix::InterruptIm1,
        Prefix::InterruptIm2,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            postcard::to_allocvec(&prefix).expect("encode old tag"),
            vec![tag as u8]
        );
    }
}
