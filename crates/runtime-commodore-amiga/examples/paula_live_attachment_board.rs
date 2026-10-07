//! Full-board reproduction of the reference's volume-attachment buffer hold.
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

fn advance<M: AmigaMachine + AmigaDriver>(machine: &mut M, channel: u16, elapsed: u32) {
    AmigaDriver::dispatch_custom_write(machine, 0x09c, 0x80 << channel);
    if elapsed == 16 {
        AmigaDriver::dispatch_custom_write(machine, 0x0aa + channel * 16, 0x3344);
        AmigaDriver::dispatch_custom_write(machine, 0x09e, 1 << channel);
    }
    AmigaMachine::tick(machine);
}

fn check<M: AmigaMachine + AmigaLiveAccess + AmigaDriver>(
    mut original: AmigaRuntime<M>,
    mut restored: AmigaRuntime<M>,
    model: &str,
    channel: u16,
) -> Result<usize, Box<dyn Error>> {
    let base = 0x0a0 + channel * 16;
    for (offset, value) in [
        (base + 6, 8),
        (base + 8, 64),
        (0x09e, 0x8000 | (1 << channel)),
        (base + 10, 0x1122),
    ] {
        AmigaDriver::dispatch_custom_write(original.machine_mut(), offset, value);
    }
    assert_eq!(
        AmigaDriver::paula(original.machine()).mix_audio_stereo(),
        (0.0, 0.0)
    );
    let mut elapsed = 0;
    let mut failed_checkpoints = 0;
    for checkpoint in [15, 16, 17, 18] {
        while elapsed < checkpoint {
            advance(original.machine_mut(), channel, elapsed);
            elapsed += 1;
        }
        let saved = original.snapshot()?;
        restored.restore(&saved)?;
        assert!(saved == restored.snapshot()?);
        let mut observations = 0;
        let mut differences = 0;
        for tick in elapsed..40 {
            advance(original.machine_mut(), channel, tick);
            advance(restored.machine_mut(), channel, tick);
            let a = AmigaDriver::paula(original.machine());
            let b = AmigaDriver::paula(restored.machine());
            assert_eq!(a.audio_diagnostic_snapshot(), b.audio_diagnostic_snapshot());
            assert_eq!(a.intreq(), b.intreq());
            assert_eq!(a.mix_audio_stereo(), b.mix_audio_stereo());
            // Reference row p=8/manual/from=volume/to=normal/edge=9:
            // the retained zero buffer stays audible-zero until high entry at 16.
            if (16..30).contains(&tick) {
                observations += 1;
                let ch = a.audio_diagnostic_snapshot().channels[usize::from(channel)];
                differences +=
                    usize::from(ch.output_sample != 0 || a.mix_audio_stereo() != (0.0, 0.0));
            }
        }
        assert!(
            observations > 0,
            "each restore must cross the audible interval"
        );
        assert!(original.snapshot()? == restored.snapshot()?);
        eprintln!(
            "{model} channel={channel} checkpoint={checkpoint}: audible mismatches={differences}/{observations}; replay identical"
        );
        failed_checkpoints += usize::from(differences != 0);
        original.restore(&saved)?;
    }
    Ok(failed_checkpoints)
}

pub fn check_all() -> Result<(), Box<dyn Error>> {
    let mut failures = 0;
    for channel in 0..4 {
        failures += check(
            AmigaOcsRuntime::new(Model::A500OcsPal, rom())?,
            AmigaOcsRuntime::new(Model::A500OcsPal, rom())?,
            "OCS",
            channel,
        )?;
        failures += check(
            AmigaEcsRuntime::new(Model::A500PlusEcsPal, rom())?,
            AmigaEcsRuntime::new(Model::A500PlusEcsPal, rom())?,
            "ECS",
            channel,
        )?;
        failures += check(
            AmigaA1200Runtime::new(Model::A1200AgaPal, rom())?,
            AmigaA1200Runtime::new(Model::A1200AgaPal, rom())?,
            "AGA",
            channel,
        )?;
    }
    eprintln!("Reference failures at {failures}/48 restore checkpoints");
    assert_eq!(failures, 0);
    Ok(())
}

#[cfg(not(test))]
fn main() -> Result<(), Box<dyn Error>> {
    check_all()
}
