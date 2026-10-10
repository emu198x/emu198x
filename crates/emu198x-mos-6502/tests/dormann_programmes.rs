//! Dormann interrupt and Bruce Clark decimal programmes, using pinned fixtures.
//! Stage with scripts/prepare-dormann-tests.py, then run this target with
//! `cargo test --release -p emu198x-mos-6502 --test dormann_programmes -- --include-ignored --nocapture`.

mod support;

use emu198x_mos_6502::M6502;

const FEEDBACK: usize = 0xbffc;
const INTERRUPT_SUCCESS: u16 = 0x0af5;
const DECIMAL_DONE: u16 = 0x024b;
const DECIMAL_ERROR: usize = 0x000b;
const DECIMAL_COMBINATIONS: u64 = 256 * 256 * 2;

#[derive(Clone, Copy)]
enum Programme {
    Interrupt,
    Decimal,
}

impl Programme {
    fn start(self) -> u16 {
        match self {
            Self::Interrupt => 0x0800,
            Self::Decimal => 0x0200,
        }
    }
}

#[derive(Debug, Default)]
struct Activity {
    cycles: u64,
    irq_assertions: u64,
    nmi_assertions: u64,
    irq_vectors: u64,
    nmi_vectors: u64,
    nmi_vectors_with_break: u64,
    decimal_adds: u64,
    decimal_subtracts: u64,
}

#[derive(Debug)]
struct RunError {
    reason: String,
    pc: u16,
    activity: Activity,
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} at ${:04X} with {:?}",
            self.reason, self.pc, self.activity
        )
    }
}

fn run(
    mem: &mut [u8; 65536],
    programme: Programme,
    decimal_enabled: bool,
    feedback_enabled: bool,
    budget: u64,
) -> Result<Activity, RunError> {
    run_with_delay(mem, programme, decimal_enabled, feedback_enabled, budget, 0)
}

fn run_with_delay(
    mem: &mut [u8; 65536],
    programme: Programme,
    decimal_enabled: bool,
    feedback_enabled: bool,
    budget: u64,
    feedback_delay: usize,
) -> Result<Activity, RunError> {
    let mut feedback_history = std::collections::VecDeque::from(vec![0; feedback_delay]);
    let mut cpu = if decimal_enabled {
        M6502::new()
    } else {
        M6502::new_2a03()
    };
    cpu.regs.pc = programme.start();
    cpu.addr = programme.start();
    cpu.sync = true;
    let mut activity = Activity::default();
    let mut previous_completed_pc = None;
    let mut last_stack_write = 0;
    mem[FEEDBACK] = 0;

    for _ in 0..budget {
        // DONE is a 65C02 STP sentinel in the supplied decimal image. Stop
        // before executing it on NMOS, but require the programme's result
        // and all 256 x 256 x 2 ADC/SBC combinations, not just the address.
        if matches!(programme, Programme::Decimal) && cpu.sync && cpu.addr == DECIMAL_DONE {
            if mem[DECIMAL_ERROR] != 0
                || activity.decimal_adds != DECIMAL_COMBINATIONS
                || activity.decimal_subtracts != DECIMAL_COMBINATIONS
            {
                return Err(RunError {
                    reason: format!("decimal result ERROR={}", mem[DECIMAL_ERROR]),
                    pc: cpu.regs.pc,
                    activity,
                });
            }
            return Ok(activity);
        }
        if cpu.sync && cpu.regs.decimal() {
            match mem[usize::from(cpu.addr)] {
                0x65 => activity.decimal_adds += 1,      // ADC zp in ADD
                0xe5 => activity.decimal_subtracts += 1, // SBC zp in SUB
                _ => {}
            }
        }
        if cpu.rw {
            cpu.data_in = mem[usize::from(cpu.addr)];
            if cpu.addr == 0xfffe {
                activity.irq_vectors += 1;
            }
            if cpu.addr == 0xfffa {
                activity.nmi_vectors += 1;
                activity.nmi_vectors_with_break += u64::from(last_stack_write & 0x10 != 0);
            }
        } else {
            mem[usize::from(cpu.addr)] = cpu.data;
            if cpu.addr & 0xff00 == 0x0100 {
                last_stack_write = cpu.data;
            }
        }
        // The programme configures an open-collector feedback register
        // without a DDR. A stored 1 asserts the corresponding input. Route
        // levels on every bus cycle; the CPU itself latches NMI edges.
        feedback_history.push_back(mem[FEEDBACK]);
        let feedback = feedback_history.pop_front().expect("feedback sample");
        let irq = feedback_enabled && feedback & 1 != 0;
        let nmi = feedback_enabled && feedback & 2 != 0;
        activity.irq_assertions += u64::from(irq && !cpu.irq);
        activity.nmi_assertions += u64::from(nmi && !cpu.nmi);
        cpu.irq = irq;
        cpu.nmi = nmi;
        let completed = cpu.tick();
        activity.cycles += 1;
        if completed && cpu.instruction_complete() {
            if previous_completed_pc == Some(cpu.regs.pc) {
                if matches!(programme, Programme::Interrupt)
                    && cpu.regs.pc == INTERRUPT_SUCCESS
                    && activity.irq_assertions > 0
                    && activity.nmi_assertions > 0
                    && activity.irq_vectors > 0
                    && activity.nmi_vectors > 0
                    && mem[0x0203] == 0
                {
                    return Ok(activity);
                }
                return Err(RunError {
                    reason: "trapped".into(),
                    pc: cpu.regs.pc,
                    activity,
                });
            }
            previous_completed_pc = Some(cpu.regs.pc);
        }
    }
    Err(RunError {
        reason: format!("exceeded {budget} cycles"),
        pc: cpu.regs.pc,
        activity,
    })
}

fn fixture(programme: Programme) -> [u8; 65536] {
    let root =
        support::find_dormann_programmes_dir().expect("stage the Dormann programme fixtures");
    let (name, origin, length) = match programme {
        Programme::Interrupt => ("6502_interrupt_test.bin", 0, 65536),
        Programme::Decimal => ("6502_decimal_test.bin", 0x0200, 258),
    };
    let bytes = std::fs::read(root.join(name)).expect("read pinned Dormann programme image");
    assert_eq!(bytes.len(), length, "wrong {name} fixture size");
    let mut mem = [0; 65536];
    mem[origin..origin + length].copy_from_slice(&bytes);
    mem
}

#[test]
#[ignore = "FIXTURE: pinned Dormann interrupt image; scripts/prepare-dormann-tests.py"]
fn interrupt_programme_with_five_cycle_feedback() {
    // This is a controlled external stimulus, not a claim about a real
    // peripheral's propagation time. Delays 0..=4 exercise the documented
    // NMOS BRK/NMI collision below. Five permits the full programme to
    // finish; six is already too late for its earlier IRQ-timeout check.
    let result = run_with_delay(
        &mut fixture(Programme::Interrupt),
        Programme::Interrupt,
        true,
        true,
        1_000_000,
        5,
    )
    .expect("Dormann interrupt programme must reach its success loop");
    eprintln!("interrupt programme passed: {result:?}");
}

#[test]
#[ignore = "FIXTURE: pinned Bruce Clark decimal image; scripts/prepare-dormann-tests.py"]
fn decimal_programme() {
    let result = run(
        &mut fixture(Programme::Decimal),
        Programme::Decimal,
        true,
        false,
        100_000_000,
    )
    .expect("Bruce Clark decimal programme must pass all operand/carry combinations");
    eprintln!("decimal programme passed: {result:?}");
}

#[test]
#[ignore = "FIXTURE: negative control needs pinned Dormann interrupt image"]
fn interrupt_programme_rejects_disconnected_feedback() {
    let result = run(
        &mut fixture(Programme::Interrupt),
        Programme::Interrupt,
        true,
        false,
        1_000_000,
    );
    assert!(
        result.is_err(),
        "disconnected IRQ/NMI feedback passed: {result:?}"
    );
    eprintln!("disconnected feedback rejected: {result:?}");
}

#[test]
#[ignore = "FIXTURE: negative control needs pinned Bruce Clark decimal image"]
fn decimal_programme_rejects_binary_only_cpu() {
    let result = run(
        &mut fixture(Programme::Decimal),
        Programme::Decimal,
        false,
        false,
        100_000_000,
    );
    assert!(result.is_err(), "binary-only arithmetic passed: {result:?}");
    eprintln!("binary-only CPU rejected: {result:?}");
}

#[test]
fn decimal_success_requires_the_operand_sweep() {
    let mut mem = [0; 65536];
    mem[0x0200..0x0203].copy_from_slice(&[0x4c, 0x4b, 0x02]);
    assert!(run(&mut mem, Programme::Decimal, true, false, 100).is_err());
}

#[test]
fn interrupt_success_requires_interrupt_activity() {
    let mut mem = [0; 65536];
    mem[0x0800..0x0803].copy_from_slice(&[0x4c, 0xf5, 0x0a]);
    mem[0x0af5..0x0af8].copy_from_slice(&[0x4c, 0xf5, 0x0a]);
    assert!(run(&mut mem, Programme::Interrupt, true, true, 100).is_err());
}

#[test]
fn unfinished_programme_exhausts_the_budget() {
    let result = run(&mut [0xea; 65536], Programme::Decimal, true, false, 10);
    assert!(
        result
            .expect_err("NOP stream cannot pass")
            .reason
            .contains("exceeded 10 cycles")
    );
}

#[test]
#[ignore = "FIXTURE: direct feedback exposes Dormann's documented NMOS BRK/NMI collision"]
fn immediate_feedback_preserves_nmi_hijacked_brk_status() {
    for delay in 0..=4 {
        let mut mem = fixture(Programme::Interrupt);
        let failure = run_with_delay(&mut mem, Programme::Interrupt, true, true, 1_000_000, delay)
            .expect_err("overlapping NMI must preserve BRK's already-pushed B bit");
        assert_eq!(failure.pc, 0x0b5c, "{failure}");
        assert_eq!(failure.reason, "trapped");
        assert_eq!(failure.activity.nmi_vectors_with_break, 1);
        assert_eq!(mem[0x0203], 7, "the concurrent IRQ/NMI/BRK test is active");
        eprintln!("feedback delay {delay}: documented NMOS collision: {failure}");
    }
}
