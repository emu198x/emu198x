//! Modulation and receiver expiry on the same CCK, across board save/restore.
use common_commodore_amiga::driver::AmigaDriver;
use emu198x_shell::MachineCore;
use runtime_commodore_amiga::{
    AmigaA1200Runtime, AmigaEcsRuntime, AmigaLiveAccess, AmigaMachine, AmigaOcsRuntime,
    AmigaRuntime, Model,
};
use std::error::Error;

fn rom() -> Vec<u8> {
    let mut bytes = vec![0; 256 * 1024];
    bytes[..10].copy_from_slice(&[0, 8, 0, 0, 0, 0xf8, 0, 8, 0x60, 0xfe]);
    bytes
}

fn advance<M: AmigaMachine + AmigaDriver>(machine: &mut M) {
    AmigaDriver::dispatch_custom_write(machine, 0x09c, 0x780);
    AmigaMachine::tick(machine);
}

fn check<M: AmigaMachine + AmigaLiveAccess + AmigaDriver>(
    mut original: AmigaRuntime<M>,
    mut restored: AmigaRuntime<M>,
    source: u16,
) -> Result<usize, Box<dyn Error>> {
    let target = usize::from(source + 1);
    for nr in [source, source + 1] {
        for (offset, value) in [(6, 8), (8, 64), (10, 0x2131)] {
            AmigaDriver::dispatch_custom_write(
                original.machine_mut(),
                0xa0 + nr * 16 + offset,
                value,
            );
        }
    }
    AmigaDriver::dispatch_custom_write(original.machine_mut(), 0x09e, 0x8000 | (0x10 << source));
    AmigaDriver::dispatch_custom_write(original.machine_mut(), 0xaa + source * 16, 1);
    let mut elapsed = 0;
    let mut failures = 0;
    for checkpoint in [14, 15, 16, 17] {
        while elapsed < checkpoint {
            advance(original.machine_mut());
            elapsed += 1;
        }
        let saved = original.snapshot()?;
        restored.restore(&saved)?;
        assert!(saved == restored.snapshot()?);
        let mut observations = 0;
        let mut differences = 0;
        for tick in elapsed..40 {
            advance(original.machine_mut());
            advance(restored.machine_mut());
            let a = AmigaDriver::paula(original.machine());
            let b = AmigaDriver::paula(restored.machine());
            assert_eq!(a.audio_diagnostic_snapshot(), b.audio_diagnostic_snapshot());
            assert_eq!(a.intreq(), b.intreq());
            assert_eq!(a.mix_audio_stereo(), b.mix_audio_stereo());
            // A board tick is half a CCK; output clocks occur at even ticks.
            // At CCK 8 both bytes expire: source sends period 1, target reloads 1.
            let cck = tick / 2 + 1;
            if cck >= 8 {
                observations += 1;
                let ch = a.audio_diagnostic_snapshot().channels[target];
                // IRQ acknowledgement precedes begin_audio_cck on this board.
                // The one-CCK low period sees a newly delivered IRQ at 10,
                // latches stop, and enters Idle at 11 holding the low sample.
                let sample = if cck < 11 && cck % 2 != 0 { 0x21 } else { 0x31 };
                // A separate constant sample verifies the audible mixer level.
                let mut expected = machine_commodore_amiga_ocs::Paula8364::new();
                expected.write_audio(
                    target as u8,
                    machine_commodore_amiga_ocs::AudioField::Vol,
                    64,
                );
                let idle = expected.audio_diagnostic_snapshot().channels[target].state;
                expected.write_audio(
                    target as u8,
                    machine_commodore_amiga_ocs::AudioField::Dat,
                    (sample as u16) << 8,
                );
                let playing = expected.audio_diagnostic_snapshot().channels[target].state;
                differences += usize::from(
                    ch.state != if cck < 11 { playing } else { idle }
                        || ch.period_counter != 1
                        || ch.output_sample != sample
                        || a.mix_audio_stereo() != expected.mix_audio_stereo(),
                );
            }
        }
        assert!(observations > 0);
        assert!(original.snapshot()? == restored.snapshot()?);
        eprintln!(
            "source={source} checkpoint={checkpoint}: mismatches={differences}/{observations}; replay identical"
        );
        failures += usize::from(differences != 0);
        original.restore(&saved)?;
    }
    Ok(failures)
}

#[test]
fn running_target_reload_and_audio_survive_live_restore() -> Result<(), Box<dyn Error>> {
    let mut failures = 0;
    for source in 0..3 {
        failures += check(
            AmigaOcsRuntime::new(Model::A500OcsPal, rom())?,
            AmigaOcsRuntime::new(Model::A500OcsPal, rom())?,
            source,
        )?;
        failures += check(
            AmigaEcsRuntime::new(Model::A500PlusEcsPal, rom())?,
            AmigaEcsRuntime::new(Model::A500PlusEcsPal, rom())?,
            source,
        )?;
        failures += check(
            AmigaA1200Runtime::new(Model::A1200AgaPal, rom())?,
            AmigaA1200Runtime::new(Model::A1200AgaPal, rom())?,
            source,
        )?;
    }
    eprintln!("Reference failures at {failures}/36 restore checkpoints");
    assert_eq!(failures, 0);
    Ok(())
}
