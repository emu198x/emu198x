//! Spectrum capture over the existing master-clock driver. No timing is inferred
//! from disassembly: every elapsed tick includes the driver's contention.

use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum_48k_class::{SpectrumMachineCore, Variant48kClass};
use emu198x_shell::{
    MachineError,
    cycle_profile::{
        CycleCounts, ExecutionCost, MappedExecutionCost, ProfileMapping, ProfileMemory,
        validate_ticks,
    },
};
use emu198x_zilog_z80::ExecutionKind;

pub(crate) trait ProfileMachine: SpectrumDriver {
    fn cpu(&self) -> &emu198x_zilog_z80::Z80;
    fn cpu_mut(&mut self) -> &mut emu198x_zilog_z80::Z80;
    fn mapping(&self, address: u16) -> Option<ProfileMapping>;
}

impl<M: MemoryBus, V: Variant48kClass> ProfileMachine for SpectrumMachineCore<M, V> {
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

impl<V: common_sinclair_zx_spectrum_amstrad_class::AmstradVariant> ProfileMachine
    for common_sinclair_zx_spectrum_amstrad_class::SpectrumAmstradClassCore<V>
{
    fn cpu(&self) -> &emu198x_zilog_z80::Z80 {
        &self.z80
    }
    fn cpu_mut(&mut self) -> &mut emu198x_zilog_z80::Z80 {
        &mut self.z80
    }
    fn mapping(&self, address: u16) -> Option<ProfileMapping> {
        use common_sinclair_zx_spectrum_amstrad_class::memory::MappedBank;
        let (memory, page) = match self.memory.mapped_bank(address) {
            MappedBank::Ram(page) => (ProfileMemory::Ram, page),
            MappedBank::Rom(page) => (ProfileMemory::Rom, page),
        };
        Some(ProfileMapping {
            memory,
            page: u16::from(page),
            slot: (address >> 14) as u8,
            base: u32::from(address & 0xc000),
        })
    }
}

macro_rules! beta_profile_machine {
    ($machine:ty, $overlay_page:expr) => {
        impl ProfileMachine for $machine {
            fn cpu(&self) -> &emu198x_zilog_z80::Z80 {
                &self.z80
            }
            fn cpu_mut(&mut self) -> &mut emu198x_zilog_z80::Z80 {
                &mut self.z80
            }
            fn mapping(&self, address: u16) -> Option<ProfileMapping> {
                let slot = (address >> 14) as u8;
                let (memory, page) = match slot {
                    0 if self.beta.trdos_paged => (ProfileMemory::RomOverlay, $overlay_page),
                    0 => (ProfileMemory::Rom, self.memory.current_rom() as u16),
                    1 => (ProfileMemory::Ram, 5),
                    2 => (ProfileMemory::Ram, 2),
                    _ => (ProfileMemory::Ram, self.memory.current_bank() as u16),
                };
                Some(ProfileMapping {
                    memory,
                    page,
                    slot,
                    base: u32::from(address & 0xc000),
                })
            }
        }
    };
}

// Pentagon has one dedicated TR-DOS ROM; Scorpion currently backs the overlay
// with one of its ordinary ROM images. Keep overlay and base-ROM identities apart.
beta_profile_machine!(machine_pentagon_128::Pentagon128, 0);
beta_profile_machine!(
    machine_scorpion_zs256::ScorpionZS256,
    u16::from(machine_scorpion_zs256::memory::MemoryScorpion::TRDOS_ROM_BANK)
);

impl ProfileMachine for machine_timex_tc2048::TimexTC2048 {
    fn cpu(&self) -> &emu198x_zilog_z80::Z80 {
        &self.z80
    }
    fn cpu_mut(&mut self) -> &mut emu198x_zilog_z80::Z80 {
        &mut self.z80
    }
    fn mapping(&self, _address: u16) -> Option<ProfileMapping> {
        None
    }
}

impl ProfileMachine for machine_timex_ts2068::TimexTS2068 {
    fn cpu(&self) -> &emu198x_zilog_z80::Z80 {
        &self.z80
    }
    fn cpu_mut(&mut self) -> &mut emu198x_zilog_z80::Z80 {
        &mut self.z80
    }
    fn mapping(&self, address: u16) -> Option<ProfileMapping> {
        use machine_timex_ts2068::memory::MemorySource;
        let slot = (address >> 13) as u8;
        let (memory, page) = match self.memory.mapped_source(address) {
            MemorySource::HomeRam => (ProfileMemory::Ram, u16::from(slot)),
            MemorySource::HomeRom => (ProfileMemory::Rom, u16::from(slot)),
            MemorySource::Exrom => (ProfileMemory::RomOverlay, 0),
            MemorySource::EmptyDock => (ProfileMemory::Unmapped, u16::from(slot)),
        };
        Some(ProfileMapping {
            memory,
            page,
            slot,
            base: u32::from(address & 0xe000),
        })
    }
}

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
    mut observer: Option<&mut dyn emu198x_shell::cycle_profile::CycleObserver>,
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
    let mut awaiting_fetch = None;
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
            awaiting_fetch = Some(machine.cpu().regs.pc);
        }
        machine.advance_halfcycles(1);
        // The machine handles M1-triggered overlays before supplying the byte.
        // Observe that actual mapping once, at the first read strobe; do not
        // call bus_request(), which consumes the machine's transaction edges.
        let cpu = machine.cpu();
        if let Some(address) = awaiting_fetch
            && !cpu.at_execution_boundary()
            && cpu.m1
            && cpu.mreq
            && cpu.rd
            && cpu.addr == address
        {
            mapping = machine.mapping(address);
            awaiting_fetch = None;
        }
        if initial_tail > 0 {
            initial_tail -= 1;
            counts.leading_partial_ticks += 1;
            continue;
        }
        pending += 1;
        let completed = machine.cpu().instructions_retired();
        if completed != retired {
            retired = completed;
            completion = Some(machine.cpu().completed_execution_event());
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
        if let Some(event) = identity
            && let Some(observer) = observer.as_deref_mut()
            && event.kind != ExecutionKind::Halt
        {
            use emu198x_shell::cycle_profile::{ProfileEvent, ProfileFlow};
            use emu198x_zilog_z80::ExecutionFlow;
            observer.observe(ProfileEvent {
                address: match event.kind {
                    ExecutionKind::Instruction(address) => Some(u32::from(address)),
                    _ => None,
                },
                mapping,
                ticks: pending,
                flow: event.flow.map(|flow| match flow {
                    ExecutionFlow::Call { return_address }
                    | ExecutionFlow::Restart { return_address } => ProfileFlow::Call {
                        return_address: u32::from(return_address),
                    },
                    ExecutionFlow::Interrupt { return_address } => ProfileFlow::Interrupt {
                        return_address: u32::from(return_address),
                    },
                    ExecutionFlow::Return | ExecutionFlow::InterruptReturn => ProfileFlow::Return,
                }),
                stack_before: u32::from(event.stack_before),
                stack_after: u32::from(event.stack_after),
                next_pc: u32::from(event.next_pc),
            });
        }
        match identity.map(|event| event.kind) {
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
