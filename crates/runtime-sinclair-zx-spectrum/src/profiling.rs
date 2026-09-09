//! Spectrum capture over the existing master-clock driver. No timing is inferred
//! from disassembly: every elapsed tick includes the driver's contention.

use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use emu198x_shell::{
    MachineError,
    cycle_profile::{
        CycleCounts, ExecutionCost, MappedExecutionCost, ProfileMapping, ProfileMemory,
        validate_ticks,
    },
};
use emu198x_zilog_z80::ExecutionKind;
use machine_sinclair_zx_spectrum_48k::Spectrum48k;

pub(crate) trait ProfileMachine: SpectrumDriver {
    fn cpu(&self) -> &emu198x_zilog_z80::Z80;
    fn cpu_mut(&mut self) -> &mut emu198x_zilog_z80::Z80;
    fn mapping(&self, address: u16) -> Option<ProfileMapping>;
}

impl ProfileMachine for Spectrum48k {
    fn cpu(&self) -> &emu198x_zilog_z80::Z80 {
        self.z80()
    }
    fn cpu_mut(&mut self) -> &mut emu198x_zilog_z80::Z80 {
        self.z80_mut()
    }
    fn mapping(&self, _address: u16) -> Option<ProfileMapping> {
        None
    }
}

macro_rules! banked_profile_machine {
    ($($machine:ty),+ $(,)?) => { $(
        impl ProfileMachine for $machine {
            fn cpu(&self) -> &emu198x_zilog_z80::Z80 { &self.z80 }
            fn cpu_mut(&mut self) -> &mut emu198x_zilog_z80::Z80 { &mut self.z80 }
            fn mapping(&self, address: u16) -> Option<ProfileMapping> {
                let slot = (address >> 14) as u8;
                let (memory, page) = match slot {
                    0 => (ProfileMemory::Rom, self.memory.current_rom()),
                    1 => (ProfileMemory::Ram, 5),
                    2 => (ProfileMemory::Ram, 2),
                    _ => (ProfileMemory::Ram, self.memory.current_bank()),
                };
                Some(ProfileMapping { memory, slot, page: u16::from(page), base: u32::from(address & 0xc000) })
            }
        }
    )+ };
}

banked_profile_machine!(
    machine_sinclair_zx_spectrum_128k::Spectrum128K,
    machine_sinclair_zx_spectrum_plus2::SpectrumPlus2,
);

// Mirror the existing driver's scheduled phases, including odd divisors.
fn ticks_to_edge(hc: u32, divisor: u32) -> u32 {
    let phase = hc % divisor;
    if phase == 0 || phase == divisor / 2 {
        0
    } else if phase < divisor / 2 {
        divisor / 2 - phase
    } else {
        divisor - phase
    }
}

pub(crate) fn capture(
    machine: &mut impl ProfileMachine,
    ticks: u32,
) -> Result<CycleCounts, MachineError> {
    validate_ticks(ticks)?;
    let mut counts = CycleCounts {
        ticks: u64::from(ticks),
        ..CycleCounts::default()
    };
    let mut retired = machine.cpu().instructions_retired();
    let mut pending = 0;
    let mut completion = None;
    // Capture may start after a retirement edge but before its half-cycle
    // has finished. Do not charge that tail to the next instruction.
    let divisor = machine.frame_timing().cpu_divisor;
    let mut mapped = std::collections::BTreeMap::<(u32, ProfileMapping), ExecutionCost>::new();
    let mut mapping = None;
    let mut initial_tail = if machine.cpu().at_execution_boundary() {
        ticks_to_edge(machine.hc(), divisor)
    } else {
        0
    };
    machine.cpu_mut().start_execution_observation();
    for _ in 0..ticks {
        // This path also wraps video frames and flushes machine audio, just
        // as ordinary execution does. Do not use the unwrapped tick helper.
        if initial_tail == 0 && completion.is_none() && machine.cpu().at_execution_boundary() {
            mapping = machine.mapping(machine.cpu().regs.pc);
        }
        machine.advance_halfcycles(1);
        if initial_tail > 0 {
            initial_tail -= 1;
            counts.leading_partial_ticks += 1;
            continue;
        }
        pending += 1;
        let completed = machine.cpu().instructions_retired();
        if completed != retired {
            retired = completed;
            completion = Some(machine.cpu().completed_execution());
        }
        // Retirement is observed on a CPU clock edge. Its half-cycle still
        // occupies the following master ticks. Close the interval at the next
        // scheduled CPU edge, before it runs, so a 4-T NOP costs 16 ticks even
        // when capture begins at reset. This observes the existing divider;
        // it neither advances the CPU nor manufactures emulated time.
        if ticks_to_edge(machine.hc(), divisor) != 0 {
            continue;
        }
        let Some(identity) = completion.take() else {
            continue;
        };
        match identity {
            Some(ExecutionKind::Instruction(address)) => {
                let cost = counts.addresses.entry(u32::from(address)).or_default();
                cost.executions += 1;
                cost.ticks += pending;
                if let Some(mapping) = mapping {
                    let cost = mapped.entry((u32::from(address), mapping)).or_default();
                    cost.executions += 1;
                    cost.ticks += pending;
                }
            }
            Some(ExecutionKind::Interrupt) => counts.interrupt_ticks += pending,
            Some(ExecutionKind::Halt) => counts.halt_ticks += pending,
            None => counts.leading_partial_ticks += pending,
        }
        pending = 0;
    }
    counts.mapped_addresses = mapped
        .into_iter()
        .map(|((address, mapping), cost)| MappedExecutionCost {
            address,
            mapping,
            cost,
        })
        .collect();
    counts.trailing_partial_ticks = pending;
    machine.cpu_mut().stop_execution_observation();
    Ok(counts)
}
