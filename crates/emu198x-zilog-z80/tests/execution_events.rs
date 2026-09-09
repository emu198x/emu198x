//! Optional call observation must preserve every serialized CPU state and bus
//! operation. Transfer hooks follow the core's existing CALL/RET/RST paths;
//! FUSE's z80_macros.h CALL/RET/RST is the reference for push/pop semantics.
use emu198x_zilog_z80::{ExecutionEvent, ExecutionFlow, ExecutionKind, Z80};

struct Pair {
    observed: Z80,
    plain: Z80,
    memory: [u8; 65536],
    plain_memory: [u8; 65536],
}

impl Pair {
    fn new(program: &[u8]) -> Self {
        let mut cpu = Z80::new();
        cpu.regs.sp = 0xff00;
        let mut pair = Self {
            observed: cpu.clone(),
            plain: cpu,
            memory: [0; 65536],
            plain_memory: [0; 65536],
        };
        pair.memory[..program.len()].copy_from_slice(program);
        pair.plain_memory = pair.memory;
        pair.observed.start_execution_observation();
        pair
    }

    fn configure(&mut self, change: impl Fn(&mut Z80)) {
        change(&mut self.observed);
        change(&mut self.plain);
    }

    fn write(&mut self, address: usize, bytes: &[u8]) {
        self.memory[address..address + bytes.len()].copy_from_slice(bytes);
        self.plain_memory[address..address + bytes.len()].copy_from_slice(bytes);
    }

    fn tick(&mut self) {
        for (cpu, memory) in [
            (&mut self.observed, &mut self.memory),
            (&mut self.plain, &mut self.plain_memory),
        ] {
            if cpu.mreq && cpu.rd {
                cpu.data_in = memory[usize::from(cpu.addr)];
            }
            if cpu.mreq && cpu.wr {
                memory[usize::from(cpu.addr)] = cpu.data;
            }
            if cpu.iorq && cpu.m1 {
                cpu.data_in = 0xff;
            }
            cpu.tick();
        }
        assert_eq!(
            serde_json::to_value(&self.observed).expect("observed state"),
            serde_json::to_value(&self.plain).expect("plain state"),
            "observation must not change a half-cycle"
        );
    }

    fn retire(&mut self) -> (Option<ExecutionEvent>, usize) {
        let before = self.observed.instructions_retired();
        for ticks in 1..=2048 {
            self.tick();
            if self.observed.instructions_retired() != before {
                assert_eq!(self.memory, self.plain_memory);
                let event = self.observed.completed_execution_event();
                assert_eq!(
                    self.observed.completed_execution(),
                    event.map(|event| event.kind)
                );
                assert_eq!(self.plain.completed_execution_event(), None);
                return (event, ticks);
            }
        }
        panic!("instruction did not retire within the guard")
    }
}

#[test]
fn calls_and_returns_preserve_first_prefix_and_wrapping_stack() {
    for prefix in [&[][..], &[0xdd][..], &[0xdd, 0xfd][..]] {
        let mut program = prefix.to_vec();
        program.extend([0xcd, 0x10, 0]);
        let mut pair = Pair::new(&program);
        pair.configure(|cpu| cpu.regs.sp = 1);
        pair.write(0x10, &[0xc9]);
        let return_address = u16::try_from(program.len()).expect("small program");
        let (event, ticks) = pair.retire();
        assert_eq!(
            event,
            Some(ExecutionEvent {
                kind: ExecutionKind::Instruction(0),
                flow: Some(ExecutionFlow::Call { return_address }),
                stack_before: 1,
                stack_after: 0xffff,
                next_pc: 0x10,
            })
        );
        assert_eq!(ticks, 34 + prefix.len() * 8);
        let (event, ticks) = pair.retire();
        assert_eq!(
            event,
            Some(ExecutionEvent {
                kind: ExecutionKind::Instruction(0x10),
                flow: Some(ExecutionFlow::Return),
                stack_before: 0xffff,
                stack_after: 1,
                next_pc: return_address,
            })
        );
        assert_eq!(ticks, 20);
    }
}

#[test]
fn every_conditional_call_and_return_reports_only_the_taken_path() {
    // Z, C, P/V and S, paired with the true sense for NZ/Z, NC/C, PO/PE, P/M.
    for (condition, flag) in [0x40, 0x40, 1, 1, 4, 4, 0x80, 0x80].into_iter().enumerate() {
        for taken in [false, true] {
            let set = taken == (condition % 2 == 1);
            let flags = if set { flag } else { 0 };
            let opcode_bits = u8::try_from(condition).expect("condition") << 3;
            // A taken CALL to its fall-through address still creates a frame.
            let mut pair = Pair::new(&[0xc4 | opcode_bits, 3, 0]);
            pair.configure(|cpu| cpu.regs.set_f(flags));
            let (event, ticks) = pair.retire();
            let event = event.expect("whole call");
            assert_eq!(
                event.flow,
                taken.then_some(ExecutionFlow::Call { return_address: 3 })
            );
            assert_eq!(event.next_pc, 3);
            assert_eq!(event.stack_after, if taken { 0xfefe } else { 0xff00 });
            assert_eq!(ticks, if taken { 34 } else { 20 });

            let mut pair = Pair::new(&[0xc0 | opcode_bits]);
            pair.configure(|cpu| cpu.regs.set_f(flags));
            pair.write(0xff00, &[0x34, 0x12]);
            let (event, ticks) = pair.retire();
            let event = event.expect("whole return");
            assert_eq!(event.flow, taken.then_some(ExecutionFlow::Return));
            assert_eq!(event.next_pc, if taken { 0x1234 } else { 1 });
            assert_eq!(event.stack_after, if taken { 0xff02 } else { 0xff00 });
            assert_eq!(ticks, if taken { 22 } else { 10 });
        }
    }
}

#[test]
fn restart_vectors_and_interrupt_return_aliases_are_distinct() {
    for vector in (0..=0x38).step_by(8) {
        let mut pair = Pair::new(&[0xc7 | vector]);
        let event = pair.retire().0.expect("restart");
        assert_eq!(
            event.flow,
            Some(ExecutionFlow::Restart { return_address: 1 })
        );
        assert_eq!(event.next_pc, u16::from(vector));
        assert_eq!(event.stack_after, 0xfefe);
    }
    for opcode in [0x45, 0x4d, 0x55, 0x5d, 0x65, 0x6d, 0x75, 0x7d] {
        let mut pair = Pair::new(&[0xed, opcode]);
        pair.write(0xff00, &[0x34, 0x12]);
        let event = pair.retire().0.expect("interrupt return");
        assert_eq!(event.flow, Some(ExecutionFlow::InterruptReturn));
        assert_eq!(event.next_pc, 0x1234);
        assert_eq!(event.stack_after, 0xff02);
    }
}

#[test]
fn interrupt_entries_capture_the_interrupted_pc_and_actual_destination() {
    for (im, nmi, destination) in [
        (0, false, 0x38),
        (1, false, 0x38),
        (2, false, 0x1234),
        (1, true, 0x66),
    ] {
        let mut pair = Pair::new(&[0]);
        pair.configure(|cpu| {
            cpu.regs.im = im;
            cpu.regs.i = 0x80;
            cpu.regs.iff1 = true;
            cpu.regs.iff2 = true;
        });
        pair.write(0x80ff, &[0x34, 0x12]);
        pair.retire();
        pair.configure(|cpu| {
            cpu.irq = !nmi;
            cpu.nmi = nmi;
        });
        let event = pair.retire().0.expect("interrupt entry");
        assert_eq!(
            event,
            ExecutionEvent {
                kind: ExecutionKind::Interrupt,
                flow: Some(ExecutionFlow::Interrupt { return_address: 1 }),
                stack_before: 0xff00,
                stack_after: 0xfefe,
                next_pc: destination,
            }
        );
    }
}

#[test]
fn partial_start_stop_and_snapshot_restore_cannot_leak_transfer_events() {
    let mut pair = Pair::new(&[0xcd, 0x10, 0]);
    pair.observed.stop_execution_observation();
    for _ in 0..8 {
        pair.tick();
    }
    pair.observed.start_execution_observation();
    assert_eq!(pair.retire().0, None, "partial CALL has no observed start");
    let event = pair.retire().0.expect("next NOP");
    assert_eq!(event.flow, None);
    assert_eq!(event.kind, ExecutionKind::Instruction(0x10));
    let saved = serde_json::to_value(&pair.observed).expect("snapshot");
    let restored: Z80 = serde_json::from_value(saved).expect("restore");
    assert_eq!(restored.completed_execution_event(), None);
    pair.observed.stop_execution_observation();
    assert_eq!(pair.observed.completed_execution_event(), None);
}

#[test]
fn ordinary_jump_and_halt_intervals_do_not_inherit_call_events() {
    let mut pair = Pair::new(&[0xcd, 0x10, 0]);
    pair.write(0x10, &[0xc3, 0x20, 0]);
    pair.write(0x20, &[0x76]);
    assert!(pair.retire().0.expect("call").flow.is_some());
    for kind in [
        ExecutionKind::Instruction(0x10),
        ExecutionKind::Instruction(0x20),
        ExecutionKind::Halt,
    ] {
        let event = pair.retire().0.expect("complete interval");
        assert_eq!(event.kind, kind);
        assert_eq!(event.flow, None);
    }
}
