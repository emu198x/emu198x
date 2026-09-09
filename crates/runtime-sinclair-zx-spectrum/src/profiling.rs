//! 48K capture over the existing master-clock driver. No timing is inferred
//! from disassembly: every elapsed tick includes the driver's contention.

use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use emu198x_shell::{
    MachineError,
    cycle_profile::{CycleCounts, validate_ticks},
};
use emu198x_zilog_z80::ExecutionKind;
use machine_sinclair_zx_spectrum_48k::Spectrum48k;

pub(crate) fn capture(machine: &mut Spectrum48k, ticks: u32) -> Result<CycleCounts, MachineError> {
    validate_ticks(ticks)?;
    let mut counts = CycleCounts {
        ticks: u64::from(ticks),
        ..CycleCounts::default()
    };
    let mut retired = machine.z80().instructions_retired();
    let mut pending = 0;
    let mut completion = None;
    // Capture may start after a retirement edge but before its half-cycle
    // has finished. Do not charge that tail to the next instruction.
    let edge_period = machine.frame_timing().cpu_divisor / 2;
    let mut initial_tail = if machine.z80().at_execution_boundary() {
        (edge_period - machine.hc() % edge_period) % edge_period
    } else {
        0
    };
    machine.z80_mut().start_execution_observation();
    for _ in 0..ticks {
        // This path also wraps video frames and flushes machine audio, just
        // as ordinary execution does. Do not use the unwrapped tick helper.
        machine.advance_halfcycles(1);
        if initial_tail > 0 {
            initial_tail -= 1;
            counts.leading_partial_ticks += 1;
            continue;
        }
        pending += 1;
        let completed = machine.z80().instructions_retired();
        if completed != retired {
            retired = completed;
            completion = Some(machine.z80().completed_execution());
        }
        // Retirement is observed on a CPU clock edge. Its half-cycle still
        // occupies the following master ticks. Close the interval at the next
        // scheduled CPU edge, before it runs, so a 4-T NOP costs 16 ticks even
        // when capture begins at reset. This observes the existing divider;
        // it neither advances the CPU nor manufactures emulated time.
        if !machine.hc().is_multiple_of(edge_period) {
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
            }
            Some(ExecutionKind::Interrupt) => counts.interrupt_ticks += pending,
            Some(ExecutionKind::Halt) => counts.halt_ticks += pending,
            None => counts.leading_partial_ticks += pending,
        }
        pending = 0;
    }
    counts.trailing_partial_ticks = pending;
    machine.z80_mut().stop_execution_observation();
    Ok(counts)
}
