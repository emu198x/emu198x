//! DDF delivery and save/restore through the installed OCS, ECS and AGA boards.
use std::error::Error;

use common_commodore_amiga::driver::AmigaDriver;
use emu198x_shell::MachineCore;
use motorola_68000::{
    bus::{BusStatus, FunctionCode},
    cpu::State,
    microcode::MicroOp,
};
use runtime_commodore_amiga::{
    AmigaA1200Runtime, AmigaEcsRuntime, AmigaLiveAccess, AmigaMachine, AmigaOcsRuntime,
    AmigaRuntime, Model,
};

fn setup<M: AmigaMachine + AmigaLiveAccess + AmigaDriver>(
    runtime: &mut AmigaRuntime<M>,
    start: u16,
    stop: u16,
) {
    let machine = runtime.machine_mut();
    machine.cpu_base_mut().state = State::Stopped;
    let a = machine.agnus_mut();
    a.vpos = 32;
    a.bplcon0 = 0x1000;
    a.ddfstrt = start;
    a.ddfstop = stop;
    a.bpl_pt[0] = 0x2000;
    machine.dispatch_custom_write(0x08e, 0x2010);
    machine.dispatch_custom_write(0x090, 0xa020);
    machine.dispatch_custom_write(0x096, 0x8300);
}

fn check_cpu_delivery<M: AmigaMachine + AmigaLiveAccess + AmigaDriver>(
    mut original: AmigaRuntime<M>,
    mut restored: AmigaRuntime<M>,
    reg: u16,
    next_phase: u8,
    wrap: bool,
) -> Result<(), Box<dyn Error>> {
    let initial_start = if reg == 0x92 { 64 } else { 56 };
    setup(&mut original, initial_start, 64);
    let target = if wrap { 226 } else { 63 };
    for _ in 0..454 {
        AmigaMachine::tick(original.machine_mut());
        if AmigaDriver::agnus(original.machine()).hpos == target
            && original.machine().cck_phase() == next_phase
        {
            break;
        }
    }
    assert_eq!(AmigaDriver::agnus(original.machine()).hpos, target);
    assert_eq!(original.machine().cck_phase(), next_phase);
    let value = if wrap {
        0
    } else if reg == 0x92 {
        64
    } else {
        128
    };
    // Deliver a mature CPU bus request using the real motherboard adapter.
    // The test controls delivery phase; it does not infer instruction latency.
    let cpu = original.machine_mut().cpu_base_mut();
    cpu.state = State::BusCycle {
        op: MicroOp::WriteWord,
        addr: 0xdff000 + u32::from(reg),
        fc: FunctionCode::SupervisorData,
        is_read: false,
        is_word: true,
        data: Some(value),
        cycle_count: 2,
    };
    cpu.bus_status = BusStatus::Wait;
    original.machine_mut().service_cpu_bus();
    assert_eq!(
        original.machine().cpu_base().bus_status,
        BusStatus::Ready(0)
    );
    original.machine_mut().cpu_base_mut().state = State::Stopped;
    let diagnostic = AmigaDriver::agnus(original.machine()).ddf_diagnostic_snapshot();
    assert_eq!(diagnostic.pending_write, Some((reg, value)));
    if reg == 0x92 {
        assert_eq!(diagnostic.effective_ddfstrt, 0xFFFF);
    } else {
        assert_eq!(diagnostic.effective_ddfstop, 64);
        assert_ne!(diagnostic.effective_ddfstop, value);
    }
    let mut pending_observations = 0;
    let mut retired_observations = 0;
    for step in 0..20 {
        let snapshot = original.snapshot()?;
        restored.restore(&snapshot)?;
        assert_eq!(snapshot, restored.snapshot()?);
        let d = AmigaDriver::agnus(original.machine()).ddf_diagnostic_snapshot();
        if d.pending_write.is_some() {
            pending_observations += 1;
        } else {
            retired_observations += 1;
        }
        AmigaMachine::tick(original.machine_mut());
        AmigaMachine::tick(restored.machine_mut());
        assert_eq!(
            original.snapshot()?,
            restored.snapshot()?,
            "step {step}, reg {reg:x}, phase {next_phase}, wrap {wrap}"
        );
    }
    assert_eq!(pending_observations, 1 + usize::from(next_phase == 1));
    assert_eq!(pending_observations + retired_observations, 20);
    let d = AmigaDriver::agnus(original.machine()).ddf_diagnostic_snapshot();
    assert_eq!(
        if reg == 0x92 {
            d.effective_ddfstrt
        } else {
            d.effective_ddfstop
        },
        value
    );
    if !wrap {
        assert_eq!(
            AmigaDriver::agnus(original.machine()).bpl_pt[0],
            if reg == 0x92 { 0x2000 } else { 0x2002 }
        );
        // Complete the last request's two physical pipeline cells.
        for _ in 0..4 {
            AmigaMachine::tick(original.machine_mut());
        }
        assert_eq!(
            AmigaDriver::agnus(original.machine()).bpl_pt[0],
            if reg == 0x92 { 0x2000 } else { 0x2004 }
        );
    }
    Ok(())
}

#[test]
fn cpu_ddf_writes_replay_at_both_phases_and_across_wrap() -> Result<(), Box<dyn Error>> {
    for reg in [0x92, 0x94] {
        for phase in 0..2 {
            for wrap in [false, true] {
                check_cpu_delivery(
                    AmigaOcsRuntime::blank(Model::A500OcsPal),
                    AmigaOcsRuntime::blank(Model::A500OcsPal),
                    reg,
                    phase,
                    wrap,
                )?;
                check_cpu_delivery(
                    AmigaEcsRuntime::blank(Model::A500PlusEcsPal),
                    AmigaEcsRuntime::blank(Model::A500PlusEcsPal),
                    reg,
                    phase,
                    wrap,
                )?;
                check_cpu_delivery(
                    AmigaA1200Runtime::blank(Model::A1200AgaPal),
                    AmigaA1200Runtime::blank(Model::A1200AgaPal),
                    reg,
                    phase,
                    wrap,
                )?;
            }
        }
    }
    Ok(())
}

#[test]
fn invalid_pending_ddf_restore_leaves_destination_untouched() -> Result<(), Box<dyn Error>> {
    fn check<M: AmigaMachine + AmigaLiveAccess + AmigaDriver>(
        mut original: AmigaRuntime<M>,
        mut destination: AmigaRuntime<M>,
    ) -> Result<(), Box<dyn Error>> {
        original.machine_mut().agnus_mut().write_ddfstop(128);
        // A pending value must match the raw mirror that produced it.
        original.machine_mut().agnus_mut().ddfstop = 64;
        let before = destination.snapshot()?;
        let error = destination
            .restore(&original.snapshot()?)
            .expect_err("malformed pending DDF value");
        assert!(
            error
                .to_string()
                .contains("invalid saved pending DDF register write"),
            "{error}"
        );
        assert_eq!(destination.snapshot()?, before);
        Ok(())
    }
    check(
        AmigaOcsRuntime::blank(Model::A500OcsPal),
        AmigaOcsRuntime::blank(Model::A500OcsPal),
    )?;
    check(
        AmigaEcsRuntime::blank(Model::A500PlusEcsPal),
        AmigaEcsRuntime::blank(Model::A500PlusEcsPal),
    )?;
    check(
        AmigaA1200Runtime::blank(Model::A1200AgaPal),
        AmigaA1200Runtime::blank(Model::A1200AgaPal),
    )
}
